//! WinEvent hooks. Installed on the main thread, whose message loop delivers
//! the callbacks.

use parking_lot::Mutex;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
};

use super::{dock, guard, taskbar};

static HOOKS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// (Re)install the hooks. Safe to call again, for example after Explorer
/// restarts and its process id changes.
pub fn install() {
    uninstall();
    let mut hooks = Vec::new();
    unsafe {
        // Foreground changes, system wide: drives fullscreen detection.
        let foreground = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if !foreground.is_invalid() {
            hooks.push(foreground.0 as isize);
        }

        // Windows shown by Explorer only: catches the taskbar reappearing
        // while "hide completely" is on.
        if let Some(pid) = taskbar::explorer_pid() {
            let shown = SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_SHOW,
                None,
                Some(callback),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if !shown.is_invalid() {
                hooks.push(shown.0 as isize);
            }
        }
    }
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
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    guard::run("a WinEvent callback", || match event {
        EVENT_SYSTEM_FOREGROUND => dock::on_foreground_changed(hwnd),
        EVENT_OBJECT_SHOW if id_object == OBJID_WINDOW.0 => taskbar::on_window_shown(hwnd),
        _ => {}
    });
}
