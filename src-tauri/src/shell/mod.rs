//! Everything that touches the Windows shell: the dock window, its AppBar
//! registration, the taskbar, and the machinery that undoes all of it.

pub mod appbar;
pub mod cover;
pub mod dock;
pub mod fullscreen;
pub mod guard;
pub mod hooks;
pub mod journal;
pub mod monitors;
pub mod subclass;
pub mod taskbar;
pub mod tray;
pub mod watchdog;
pub mod win;
pub mod worker;

use std::sync::OnceLock;
use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, PostMessageW, RegisterWindowMessageW,
};
use windows::core::w;

use crate::filelog;

/// Broadcast by Explorer when it (re)creates the taskbar, for example after
/// an Explorer restart. Every appbar registration is gone at that point.
pub fn taskbar_created_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

/// Sent by `glassdock.exe --restore` to a running dock, telling it to hand the
/// taskbar back and stop managing it.
pub fn restore_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(w!("Glassdock.RestoreTaskbar")) })
}

/// Sent by `glassdock.exe --quit` to a running dock, telling it to exit the
/// same way the menu's Quit does. The installer uses this before it replaces
/// or removes files.
pub fn quit_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(w!("Glassdock.Quit")) })
}

/// `glassdock.exe --quit`: ask a running dock to exit cleanly.
pub fn quit_command() {
    unsafe {
        let _ = PostMessageW(Some(HWND_BROADCAST), quit_message(), WPARAM(0), LPARAM(0));
    }
    filelog::line("quit", "--quit sent to any running dock");
}

/// `glassdock.exe --restore`: the kill switch. Works whether or not a dock is
/// running, and whether or not a journal exists.
pub fn restore_command() {
    // A running dock in "hide completely" mode would hide the taskbar again,
    // so ask it to let go first. The message is unique to Glassdock; every
    // other window ignores it.
    unsafe {
        let _ = PostMessageW(
            Some(HWND_BROADCAST),
            restore_message(),
            WPARAM(0),
            LPARAM(0),
        );
    }
    std::thread::sleep(Duration::from_millis(500));

    let from_journal = taskbar::restore_from_journal();
    taskbar::force_show();
    filelog::line(
        "restore",
        &format!(
            "--restore ran; state restored from journal: {from_journal}; taskbar windows shown"
        ),
    );
}
