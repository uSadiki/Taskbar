//! The watchdog: a windowless copy of the executable that outlives a crash.
//!
//! The dock starts `--watchdog-launcher <pid>`, which starts `--watchdog <pid>`
//! and exits. Going through a launcher detaches the watchdog from the dock's
//! process tree, so a tree kill (`taskkill /T`, a dev tool stopping its child
//! processes) takes down the dock but not the thing that cleans up after it.

use std::os::windows::process::CommandExt;
use std::process::Command;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

use super::{journal, taskbar};
use crate::filelog;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

fn spawn_self(args: &[String]) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    // Prefer leaving any job object the parent is in; if the job forbids
    // breakaway the first attempt fails and the plain spawn is used.
    let attempt = Command::new(&exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB)
        .spawn();
    match attempt {
        Ok(_) => Ok(()),
        Err(_) => Command::new(&exe)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ()),
    }
}

/// Called by the dock at startup, before it changes the taskbar.
pub fn spawn_for_current_process() {
    let pid = std::process::id().to_string();
    if let Err(e) = spawn_self(&["--watchdog-launcher".into(), pid]) {
        log::error!("watchdog: could not start the launcher: {e}");
        filelog::line("watchdog", &format!("could not start the launcher: {e}"));
    }
}

/// `--watchdog-launcher <pid>`: start the real watchdog and return.
pub fn launch_detached(pid: u32) {
    if let Err(e) = spawn_self(&["--watchdog".into(), pid.to_string()]) {
        filelog::line(
            "watchdog",
            &format!("could not start the watchdog for pid {pid}: {e}"),
        );
    }
}

/// `--watchdog <pid>`: block until that process exits, then restore the taskbar
/// if the process left its journal behind.
pub fn run(pid: u32) {
    match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        Ok(handle) => unsafe {
            WaitForSingleObject(handle, INFINITE);
            let _ = CloseHandle(handle);
        },
        // The process is already gone; fall through to the journal check.
        Err(e) => filelog::line("watchdog", &format!("pid {pid} could not be opened ({e})")),
    }

    match journal::load() {
        Some(entry) if entry.pid == pid => {
            let restored = taskbar::restore_from_journal();
            filelog::line(
                "watchdog",
                &format!("pid {pid} exited without cleaning up; taskbar restored: {restored}"),
            );
        }
        // No journal: clean exit. Another pid: a newer instance owns the taskbar.
        _ => {}
    }
}
