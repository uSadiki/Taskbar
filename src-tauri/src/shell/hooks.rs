//! WinEvent hooks. Installed on the main thread, whose message loop delivers
//! the callbacks.

use parking_lot::Mutex;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_CLOAKED, EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE, EVENT_OBJECT_NAMECHANGE,
    EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND,
    EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MOVESIZEEND, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
};

use super::{dock, guard, taskbar};
use crate::widgets::apps;

static HOOKS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// The event ranges the dock listens to. Kept narrow on purpose: the range
/// between HIDE and NAMECHANGE holds LOCATIONCHANGE, which fires on every
/// mouse move.
const RANGES: &[(u32, u32)] = &[
    // Foreground changes: fullscreen detection and the active-app highlight.
    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
    // A window was dragged or resized: it may now overlap the dock.
    (EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZEEND),
    (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
    // Windows appearing and disappearing (create, destroy, show, hide).
    (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
    // Title changes.
    (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
    // Store apps and virtual desktops hide windows by cloaking them.
    (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
];

/// (Re)install the hooks. Safe to call again.
pub fn install() {
    uninstall();
    let hooks = RANGES
        .iter()
        .filter_map(|&(first, last)| {
            let hook = unsafe {
                SetWinEventHook(
                    first,
                    last,
                    None,
                    Some(callback),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            (!hook.is_invalid()).then_some(hook.0 as isize)
        })
        .collect();
    *HOOKS.lock() = hooks;
}

pub fn uninstall() {
    let hooks = std::mem::take(&mut *HOOKS.lock());
    for hook in hooks {
        unsafe {
            let _ = UnhookWinEvent(HWINEVENTHOOK(hook as *mut _));
        }
    }
}

unsafe extern "system" fn callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // Only events about a window itself, not about something inside it.
    if id_object != OBJID_WINDOW.0 || id_child != 0 {
        return;
    }
    guard::run("a WinEvent callback", || match event {
        EVENT_SYSTEM_FOREGROUND => {
            dock::on_foreground_changed(hwnd);
            dock::on_windows_rearranged();
            apps::on_window_event(hwnd, false);
        }
        EVENT_SYSTEM_MOVESIZEEND => dock::on_windows_rearranged(),
        EVENT_SYSTEM_MINIMIZESTART | EVENT_SYSTEM_MINIMIZEEND => {
            dock::on_windows_rearranged();
            apps::on_window_event(hwnd, true);
        }
        EVENT_OBJECT_SHOW => {
            taskbar::on_window_shown(hwnd);
            apps::on_window_event(hwnd, true);
        }
        _ => apps::on_window_event(hwnd, true),
    });
}
