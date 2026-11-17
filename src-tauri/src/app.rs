//! The dock application: startup, shutdown and crash handling.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::ThreadId;

use tauri::window::{Effect, EffectsBuilder};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};

use crate::error::AppResult;
use crate::settings::{Settings, SettingsStore};
use crate::shell::{appbar, dock, guard, hooks, journal, subclass, taskbar, tray, watchdog};
use crate::{commands, filelog, paths};

static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();
static SAFE_MODE: AtomicBool = AtomicBool::new(false);

pub fn run(safe: bool) {
    SAFE_MODE.store(safe, Ordering::SeqCst);
    install_crash_handlers();
    recover_from_previous_run(safe);

    let log_plugin = tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(Target::new(TargetKind::Folder {
            path: paths::log_dir(),
            file_name: Some("glassdock".into()),
        }))
        .target(Target::new(TargetKind::Stdout))
        .rotation_strategy(RotationStrategy::KeepSome(3))
        .max_file_size(2 * 1024 * 1024)
        .level(log::LevelFilter::Info)
        .level_for(
            "glassdock_lib",
            if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            },
        )
        .build();

    let built = tauri::Builder::default()
        // Must be the first plugin. A second launch opens Settings in the
        // running instance and exits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            run_action(app, "settings");
        }))
        .plugin(log_plugin)
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("Glassdock")
                .build(),
        )
        .manage(SettingsStore::load(paths::settings_path()))
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::update_settings,
            commands::get_dock_layout,
            commands::set_hit_regions,
            commands::dock_drag_start,
            commands::dock_drag_move,
            commands::dock_drag_end,
            commands::dock_focus,
            commands::dock_release_focus,
            commands::glass_info,
            commands::glass_backdrop,
            commands::list_monitors,
            commands::apps_list,
            commands::apps_icon,
            commands::apps_activate,
            commands::apps_launch,
            commands::apps_window_action,
            commands::apps_close_all,
            commands::media_state,
            commands::media_art,
            commands::media_command,
            commands::battery_state,
            commands::toggles_snapshot,
            commands::radio_set,
            commands::audio_set_volume,
            commands::audio_set_muted,
            commands::audio_set_device,
            commands::brightness_set,
            commands::dark_mode_set,
            commands::weather_state,
            commands::weather_search,
            commands::weather_locate,
            commands::weather_refresh,
            commands::github_state,
            commands::github_set_token,
            commands::github_clear_token,
            commands::github_refresh,
            commands::claude_state,
            commands::open_url,
            commands::app_info,
            commands::open_settings,
            commands::open_glass_lab,
            commands::restore_taskbar,
            commands::reload_app,
            commands::quit_app,
        ])
        .setup(move |app| {
            setup(app.handle(), safe)?;
            Ok(())
        })
        .build(tauri::generate_context!());

    match built {
        Ok(app) => app.run(|_app, event| {
            if let RunEvent::Exit = event {
                shutdown();
            }
        }),
        Err(e) => {
            filelog::line("startup", &format!("Glassdock could not start: {e}"));
            shutdown();
            std::process::exit(1);
        }
    }
}

fn setup(app: &AppHandle, safe: bool) -> AppResult<()> {
    let window = WebviewWindowBuilder::new(app, "dock", WebviewUrl::App("index.html".into()))
        .title("Glassdock")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .focused(false)
        .visible(false)
        .data_directory(paths::webview_dir())
        .build()?;

    // The dock is never closed by the user; quitting goes through `quit`.
    window.on_window_event(|event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
        }
    });

    let hwnd = window.hwnd()?;
    dock::init(app.clone(), hwnd, safe);
    dock::apply_window_styles(hwnd);
    if !subclass::install(hwnd) {
        log::error!("shell: could not subclass the dock window; shell notifications are off");
    }

    if safe {
        log::warn!("safe mode: no AppBar registration, no taskbar changes");
    } else {
        // Order matters: the watchdog exists before anything is changed, and
        // `apply_taskbar_mode` writes the journal before it touches the taskbar.
        watchdog::spawn_for_current_process();
        appbar::register(hwnd);
        dock::apply_taskbar_mode();
    }

    dock::reposition();
    dock::show();
    crate::glass::start_watching(app);
    crate::widgets::apps::start(app);
    crate::widgets::media::start(app);
    crate::widgets::battery::start(app, hwnd);
    crate::widgets::toggles::start(app);
    crate::widgets::weather::start(app);
    crate::widgets::github::start(app);
    crate::widgets::claude::start(app);
    crate::autostart::sync(app, app.state::<SettingsStore>().get().general.autostart);
    hooks::install();
    tray::create(app)?;
    log::info!(
        "Glassdock {} started (safe mode: {safe})",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}

/// Undo everything Glassdock changed in Windows. Runs once; later calls are
/// no-ops, so every exit path can call it.
pub fn shutdown() {
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    hooks::uninstall();
    if let Some(rt) = dock::runtime()
        && !rt.safe
    {
        appbar::unregister(rt.hwnd());
        let restored = taskbar::restore_from_journal();
        log::info!("shutdown: AppBar removed; taskbar restored from journal: {restored}");
    }
}

pub fn quit(app: &AppHandle) {
    shutdown();
    app.exit(0);
}

/// Tray and dock menu actions.
pub fn run_action(app: &AppHandle, id: &str) {
    match id {
        "settings" => {
            // Window creation must not happen on the main thread's call stack.
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = open_settings(&app) {
                    log::error!("settings: could not open the window: {e}");
                }
            });
        }
        "restore" | "reload" | "quit" => {
            let id = id.to_owned();
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || match id.as_str() {
                "restore" => dock::suspend_taskbar(),
                "reload" => dock::reload(),
                _ => quit(&handle),
            });
        }
        other => log::warn!("unknown menu action: {other}"),
    }
}

pub fn open_settings(app: &AppHandle) -> AppResult<()> {
    open_window(
        app,
        "settings",
        "Glassdock Settings",
        (880.0, 620.0),
        (720.0, 480.0),
    )
}

/// The glass tuning panel. Only reachable from dev builds' dock menu.
pub fn open_glass_lab(app: &AppHandle) -> AppResult<()> {
    open_window(app, "glasslab", "Glass lab", (380.0, 780.0), (320.0, 400.0))
}

/// Show an ordinary (non-dock) window, creating it on first use. These
/// windows are destroyed when closed, so they cost nothing while unused.
fn open_window(
    app: &AppHandle,
    label: &str,
    title: &str,
    size: (f64, f64),
    min_size: (f64, f64),
) -> AppResult<()> {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(size.0, size.1)
        .min_inner_size(min_size.0, min_size.1)
        .center()
        .data_directory(paths::webview_dir())
        // Windows 11's Mica material behind the page, which is transparent.
        .transparent(true)
        .effects(EffectsBuilder::new().effect(Effect::Mica).build())
        .build()?;
    Ok(())
}

/// React to a settings change: tell every window, then update the shell.
pub fn settings_changed(app: &AppHandle, old: &Settings, new: &Settings) {
    let _ = app.emit("settings://changed", new.clone());

    if old.general.taskbar_mode != new.general.taskbar_mode {
        dock::resume_taskbar();
    }
    // Edge, monitor, size, layout and hiding for windows all change the
    // strip the dock reserves.
    let moved = old.general.position != new.general.position
        || old.general.monitor != new.general.monitor
        || old.general.layout != new.general.layout
        || old.general.hide_for_windows != new.general.hide_for_windows
        || old.appearance.dock_size != new.appearance.dock_size;
    if moved {
        dock::reposition();
    }
    if old.weather != new.weather {
        crate::widgets::weather::refresh();
    }
    if old.general.autostart != new.general.autostart {
        crate::autostart::sync(app, new.general.autostart);
    }
    if old.claude != new.claude {
        crate::widgets::claude::sync_bridge(new.claude.statusline_bridge);
    }
    // Leaving quiet time lets held alerts through.
    if old.notifications != new.notifications {
        crate::widgets::claude::request_refresh();
    }
}

// ---------------------------------------------------------------------------
// Crash handling
// ---------------------------------------------------------------------------

fn process_is_running(pid: u32) -> bool {
    const WAIT_TIMEOUT: u32 = 258;
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => {
                let running = WaitForSingleObject(handle, 0).0 == WAIT_TIMEOUT;
                let _ = CloseHandle(handle);
                running
            }
            Err(_) => false,
        }
    }
}

/// A journal left by a process that no longer exists means a previous run died
/// and its watchdog didn't get to clean up (power loss, for example). Put the
/// taskbar back before doing anything else. A journal owned by a live process
/// belongs to a running dock and is left alone.
fn recover_from_previous_run(safe: bool) {
    let Some(entry) = journal::load() else { return };
    if entry.pid != std::process::id() && process_is_running(entry.pid) {
        return;
    }
    let restored = taskbar::restore_from_journal();
    filelog::line(
        "startup",
        &format!(
            "leftover journal from pid {} found (safe mode: {safe}); taskbar restored: {restored}",
            entry.pid
        ),
    );
}

fn emergency_restore(reason: &str) {
    static DONE: AtomicBool = AtomicBool::new(false);
    if SAFE_MODE.load(Ordering::SeqCst) || DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    let restored = taskbar::restore_from_journal();
    filelog::line("crash", &format!("{reason}; taskbar restored: {restored}"));
}

unsafe extern "system" fn exception_filter(_info: *const EXCEPTION_POINTERS) -> i32 {
    emergency_restore("unhandled exception");
    0 // EXCEPTION_CONTINUE_SEARCH: let Windows handle the crash as usual.
}

fn install_crash_handlers() {
    let _ = MAIN_THREAD.set(std::thread::current().id());

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Only the plain file log here: the main logger may be what panicked.
        filelog::line("panic", &info.to_string());
        // A panic on the main thread outside a guarded callback ends the
        // process. Panics elsewhere are contained and the dock keeps running.
        let on_main = MAIN_THREAD.get() == Some(&std::thread::current().id());
        if on_main && !guard::in_guarded_callback() {
            emergency_restore("panic on the main thread");
        }
        previous(info);
    }));

    unsafe {
        SetUnhandledExceptionFilter(Some(exception_filter));
    }
}
