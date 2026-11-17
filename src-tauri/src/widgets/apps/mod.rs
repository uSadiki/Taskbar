//! The Apps widget: pinned apps plus every open window, grouped by app.
//!
//! All shell work runs on the shell worker thread. Window events arrive on
//! the main thread and only ask for a refresh; requests made before the
//! refresh runs collapse into one.

pub mod icons;
pub mod model;
pub mod pins;
pub mod windows;

use std::collections::HashMap;
use std::mem::size_of;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use ::windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, RegNotifyChangeKeyValue,
    RegOpenKeyExW,
};
use ::windows::Win32::System::Threading::{
    AttachThreadInput, CreateEventW, GetCurrentThreadId, INFINITE, WaitForSingleObject,
};
use ::windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_IDLIST, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GA_ROOT, GetAncestor, GetForegroundWindow, GetWindowThreadProcessId,
    IsIconic, IsWindow, PostMessageW, SC_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow,
    ShowWindow, WM_CLOSE, WM_SYSCOMMAND,
};
use ::windows::core::{HSTRING, PCWSTR, w};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use self::model::{AppGroup, ClickAction, WindowMeta};
use self::pins::{Pin, ShellRef};
use self::windows::Identity;
use crate::shell::win::{hwnd_from_isize, hwnd_to_isize};
use crate::shell::{dock, worker};

#[derive(Default)]
struct State {
    pins: Vec<Pin>,
    pins_loaded: bool,
    /// Per-window identity, read once when the window first appears.
    identities: HashMap<isize, Identity>,
    /// Windows in the order they first appeared, which fixes icon order.
    order: Vec<isize>,
    /// The last foreground window that was not the dock itself.
    foreground: isize,
    groups: Vec<AppGroup>,
    /// Where each group's icon and launch target come from.
    sources: HashMap<String, ShellRef>,
    icons: HashMap<String, Vec<u8>>,
    watcher: Option<RecommendedWatcher>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();
static REFRESH_QUEUED: AtomicBool = AtomicBool::new(false);
static PINS_DIRTY: AtomicBool = AtomicBool::new(true);

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

/// Start tracking windows and pins.
pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    watch_pins();
    request_refresh();
}

/// Ask for a refresh soon. Callable from any thread, as often as needed.
pub fn request_refresh() {
    if REFRESH_QUEUED.swap(true, Ordering::SeqCst) {
        return;
    }
    worker::post(|| {
        // Window events come in bursts (an app opening creates dozens);
        // wait for the burst to pass and handle it once.
        std::thread::sleep(Duration::from_millis(60));
        REFRESH_QUEUED.store(false, Ordering::SeqCst);
        refresh();
    });
}

/// A window event from the WinEvent hook (main thread).
pub fn on_window_event(hwnd: HWND, top_level_only: bool) {
    if hwnd.is_invalid() {
        return;
    }
    // Events also fire for every child control; only whole windows matter.
    if top_level_only && unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
        return;
    }
    request_refresh();
}

fn refresh() {
    if PINS_DIRTY.swap(false, Ordering::SeqCst) {
        let pins = pins::read();
        let mut state = state().lock();
        if !state.pins_loaded || pins.len() != state.pins.len() {
            log::info!("apps: {} pinned app(s)", pins.len());
            for pin in &pins {
                log::debug!(
                    "apps: pin {:?} identity {:?} target {:?}",
                    pin.meta.name,
                    pin.meta.aumid,
                    pin.meta.target
                );
            }
        }
        state.pins = pins;
        state.pins_loaded = true;
        // Pins may have new shortcuts behind the same id.
        state.icons.retain(|id, _| !id.starts_with("pin:"));
    }

    let handles = windows::enumerate();
    let dock_hwnd = dock::runtime()
        .map(|rt| hwnd_to_isize(rt.hwnd()))
        .unwrap_or(0);
    let foreground = hwnd_to_isize(unsafe { GetForegroundWindow() });

    let (changed, groups) = {
        let mut state = state().lock();

        // Forget closed windows, remember new ones in arrival order.
        state.order.retain(|h| handles.contains(h));
        state.identities.retain(|h, _| handles.contains(h));
        for handle in &handles {
            if !state.order.contains(handle) {
                state.order.push(*handle);
            }
            if !state.identities.contains_key(handle) {
                let identity = windows::identity(hwnd_from_isize(*handle));
                state.identities.insert(*handle, identity);
            }
        }
        // While a popover has focus, keep showing the app that was in front.
        if foreground != dock_hwnd {
            state.foreground = foreground;
        }

        let metas: Vec<WindowMeta> = state
            .order
            .iter()
            .filter_map(|handle| {
                let identity = state.identities.get(handle)?;
                let hwnd = hwnd_from_isize(*handle);
                Some(WindowMeta {
                    hwnd: *handle,
                    title: windows::title(hwnd),
                    exe: identity.exe.clone(),
                    aumid: identity.aumid.clone(),
                    app_name: identity.app_name.clone(),
                    minimized: windows::is_minimized(hwnd),
                })
            })
            .collect();
        let pin_metas: Vec<_> = state.pins.iter().map(|p| p.meta.clone()).collect();
        let groups = model::build_groups(&pin_metas, &metas, state.foreground);

        // Where each group's icon comes from: the pin itself, a packaged
        // app's Apps-folder entry, or the executable.
        let mut sources: HashMap<String, ShellRef> = HashMap::new();
        for pin in &state.pins {
            sources.insert(model::pin_id(&pin.meta), pin.shell.clone());
        }
        for meta in &metas {
            let id = model::window_key(meta);
            sources.entry(id).or_insert_with(|| match &meta.aumid {
                Some(aumid)
                    if meta.exe.to_ascii_lowercase().contains("windowsapps")
                        || meta
                            .exe
                            .to_ascii_lowercase()
                            .ends_with("applicationframehost.exe") =>
                {
                    ShellRef::Path(format!(r"shell:AppsFolder\{aumid}"))
                }
                _ => ShellRef::Path(meta.exe.clone()),
            });
        }
        state.sources = sources;

        let changed = state.groups != groups;
        state.groups = groups.clone();
        (changed, groups)
    };

    if changed && let Some(app) = APP.get() {
        let _ = app.emit("apps://changed", groups);
    }
}

pub fn groups() -> Vec<AppGroup> {
    state().lock().groups.clone()
}

/// PNG icon for a group. Rendered on first use, then served from memory.
pub fn icon(id: String) -> Option<Vec<u8>> {
    if let Some(cached) = state().lock().icons.get(&id) {
        return Some(cached.clone());
    }
    let source = state().lock().sources.get(&id).cloned()?;
    let key = id.clone();
    let png = worker::call(move || match icons::render(&source) {
        Ok(png) => Some(png),
        Err(e) => {
            log::warn!("apps: no icon for {key}: {e}");
            None
        }
    })??;
    state().lock().icons.insert(id, png.clone());
    Some(png)
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Bring a window to the front, restoring it first if it is minimized.
fn focus_window(hwnd: HWND) {
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd {
            return;
        }
        // Windows only lets the process that last received input change the
        // foreground window. The click went to the dock's webview process,
        // so borrow the foreground thread's input state for the call.
        let current = GetCurrentThreadId();
        let front = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let attached =
            front != 0 && front != current && AttachThreadInput(current, front, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(current, front, false);
        }
    }
}

fn minimize_window(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            WM_SYSCOMMAND,
            WPARAM(SC_MINIMIZE as usize),
            LPARAM(0),
        );
    }
}

fn close_window(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

fn shell_open(source: &ShellRef) {
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // Keep the string alive until the call returns.
    let path;
    match source {
        ShellRef::IdList(id_list) => {
            info.fMask |= SEE_MASK_IDLIST;
            info.lpIDList = id_list.as_ptr() as *mut _;
        }
        ShellRef::Path(value) => {
            path = HSTRING::from(value.as_str());
            info.lpFile = PCWSTR(path.as_ptr());
        }
    }
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        log::warn!("apps: could not launch: {e}");
    }
}

/// Start the app, or another instance of it.
pub fn launch(id: String) {
    let Some(source) = state().lock().sources.get(&id).cloned() else {
        return;
    };
    worker::post(move || shell_open(&source));
}

/// Left click on an app icon.
pub fn activate(id: String) {
    worker::post(move || {
        // Decide from the live foreground window, not the last refresh.
        let dock_hwnd = dock::runtime()
            .map(|rt| hwnd_to_isize(rt.hwnd()))
            .unwrap_or(0);
        let live = hwnd_to_isize(unsafe { GetForegroundWindow() });
        let (group, source) = {
            let state = state().lock();
            let foreground = if live == dock_hwnd {
                state.foreground
            } else {
                live
            };
            let mut group = state.groups.iter().find(|g| g.id == id).cloned();
            if let Some(group) = &mut group {
                for window in &mut group.windows {
                    window.active = window.hwnd == foreground;
                }
            }
            (group, state.sources.get(&id).cloned())
        };
        let Some(group) = group else { return };
        match model::click_action(&group) {
            ClickAction::Launch => {
                if let Some(source) = source {
                    shell_open(&source);
                }
            }
            ClickAction::Focus(hwnd) => focus_window(hwnd_from_isize(hwnd)),
            ClickAction::Minimize(hwnd) => minimize_window(hwnd_from_isize(hwnd)),
        }
    });
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowAction {
    Focus,
    Minimize,
    Close,
}

pub fn window_action(hwnd: isize, action: WindowAction) {
    // Only act on windows the dock is showing, never on an arbitrary handle.
    if !state().lock().order.contains(&hwnd) {
        return;
    }
    worker::post(move || {
        let hwnd = hwnd_from_isize(hwnd);
        match action {
            WindowAction::Focus => focus_window(hwnd),
            WindowAction::Minimize => minimize_window(hwnd),
            WindowAction::Close => close_window(hwnd),
        }
    });
}

pub fn close_all(id: String) {
    let handles: Vec<isize> = state()
        .lock()
        .groups
        .iter()
        .find(|g| g.id == id)
        .map(|g| g.windows.iter().map(|w| w.hwnd).collect())
        .unwrap_or_default();
    worker::post(move || {
        handles
            .into_iter()
            .for_each(|h| close_window(hwnd_from_isize(h)))
    });
}

// ---------------------------------------------------------------------------
// Pin changes
// ---------------------------------------------------------------------------

fn pins_changed() {
    PINS_DIRTY.store(true, Ordering::SeqCst);
    request_refresh();
}

/// Notice pinning and unpinning in Windows: the pin folder gains or loses a
/// shortcut, and the pin list in the registry is rewritten.
fn watch_pins() {
    if let Some(dir) = pins::pin_folder() {
        let watcher = notify::recommended_watcher(|event: notify::Result<notify::Event>| {
            if event.is_ok() {
                pins_changed();
            }
        })
        .and_then(|mut watcher| {
            watcher.watch(&dir, RecursiveMode::NonRecursive)?;
            Ok(watcher)
        });
        match watcher {
            Ok(watcher) => state().lock().watcher = Some(watcher),
            Err(e) => log::warn!("apps: cannot watch the pin folder: {e}"),
        }
    }

    let spawned = std::thread::Builder::new()
        .name("pin-watcher".into())
        .spawn(|| unsafe {
            let mut key = HKEY::default();
            let opened = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Taskband"),
                None,
                KEY_NOTIFY,
                &mut key,
            );
            let Ok(event) = CreateEventW(None, false, false, PCWSTR::null()) else {
                return;
            };
            if opened.is_err() {
                let _ = CloseHandle(event);
                return;
            }
            // Read-only: this only asks to be told when the value changes.
            while RegNotifyChangeKeyValue(key, false, REG_NOTIFY_CHANGE_LAST_SET, Some(event), true)
                .is_ok()
            {
                WaitForSingleObject(event, INFINITE);
                pins_changed();
            }
        });
    if let Err(e) = spawned {
        log::warn!("apps: cannot watch the pin list: {e}");
    }
}
