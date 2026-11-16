//! Hooks the dock window's message stream. Tauri owns the window procedure;
//! this subclass sits in front of it to see the shell messages an appbar has
//! to answer.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{
    ABN_FULLSCREENAPP, ABN_POSCHANGED, ABN_STATECHANGE, DefSubclassProc, RemoveWindowSubclass,
    SetWindowSubclass,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MA_NOACTIVATE, WA_INACTIVE, WM_ACTIVATE, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENDSESSION,
    WM_MOUSEACTIVATE, WM_NCDESTROY, WM_WINDOWPOSCHANGED,
};

use super::{appbar, dock, guard, restore_message, taskbar_created_message};

const SUBCLASS_ID: usize = 0x474C_4153; // "GLAS"

/// Must be called on the thread that owns the window.
pub fn install(hwnd: HWND) -> bool {
    // Register the shell messages up front so the proc only compares numbers.
    let _ = taskbar_created_message();
    let _ = restore_message();
    unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) }.as_bool()
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let handled = guard::run("the dock window procedure", || {
        handle(hwnd, msg, wparam, lparam)
    });
    match handled.flatten() {
        Some(result) => result,
        None => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

/// Returns `Some` to answer the message here, `None` to pass it on to Tauri.
fn handle(hwnd: HWND, msg: u32, wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
    if msg == appbar::CALLBACK_MSG {
        match wparam.0 as u32 {
            // The taskbar or another appbar moved: the strip may need to move.
            ABN_POSCHANGED => dock::request_reposition(),
            // A fullscreen app opened, or the last one closed.
            ABN_FULLSCREENAPP => dock::evaluate_fullscreen(),
            ABN_STATECHANGE => log::debug!("appbar: taskbar state changed"),
            _ => {}
        }
        return Some(LRESULT(0));
    }
    if msg == dock::WM_APP_REPOSITION {
        dock::on_reposition_message();
        return Some(LRESULT(0));
    }
    if msg == taskbar_created_message() {
        dock::on_taskbar_created();
        return None;
    }
    if msg == restore_message() {
        dock::suspend_taskbar();
        return Some(LRESULT(0));
    }

    match msg {
        WM_ACTIVATE => {
            appbar::activate(hwnd);
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                dock::on_deactivated();
            }
            None
        }
        WM_WINDOWPOSCHANGED => {
            appbar::window_pos_changed(hwnd);
            None
        }
        // Resolution, monitor set or scaling changed. Tauri handles the
        // message first (it resizes for the new DPI); the posted reposition
        // then puts the dock where it belongs.
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            dock::request_reposition();
            None
        }
        // Clicking the dock must not activate it. Menus take focus explicitly.
        WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
        // The session is ending (sign-out, shutdown): hand the taskbar back.
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                crate::app::shutdown();
            }
            None
        }
        WM_NCDESTROY => {
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
            }
            None
        }
        _ => None,
    }
}
