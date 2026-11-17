//! Controls the Windows taskbar while the dock runs, and puts it back.
//!
//! Nothing here holds a lock or depends on Tauri, so `restore_from_journal`
//! is safe to call from the panic hook, the exception filter, the watchdog
//! process and `--restore`.

use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Shell::{ABM_GETSTATE, ABM_SETSTATE, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, IsWindowVisible, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow,
};
use windows::core::{PCWSTR, w};

use super::journal::{self, TaskbarJournal};
use super::win::class_name;
use crate::error::{AppError, AppResult};
use crate::settings::TaskbarMode;

const PRIMARY_CLASS: &str = "Shell_TrayWnd";
const SECONDARY_CLASS: &str = "Shell_SecondaryTrayWnd";

/// True while "hide completely" is active: Explorer re-shows the taskbar on
/// its own now and then, and the WinEvent hook hides it again.
static HIDE_ENFORCED: AtomicBool = AtomicBool::new(false);

/// Re-hide rate limiter, so a disagreement with Explorer can never spin.
static REHIDE_WINDOW_START: AtomicU64 = AtomicU64::new(0);
static REHIDE_COUNT: AtomicU32 = AtomicU32::new(0);
const MAX_REHIDES_PER_SECOND: u32 = 20;

pub fn primary_tray() -> Option<HWND> {
    unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }
        .ok()
        .filter(|hwnd| !hwnd.is_invalid())
}

/// The primary taskbar and the taskbars on other monitors.
pub fn tray_windows() -> Vec<HWND> {
    let mut windows: Vec<HWND> = primary_tray().into_iter().collect();
    let mut previous: Option<HWND> = None;
    // Bounded loop: one secondary taskbar per extra monitor.
    for _ in 0..32 {
        let found =
            unsafe { FindWindowExW(None, previous, w!("Shell_SecondaryTrayWnd"), PCWSTR::null()) };
        match found {
            Ok(hwnd) if !hwnd.is_invalid() => {
                windows.push(hwnd);
                previous = Some(hwnd);
            }
            _ => break,
        }
    }
    windows
}

pub fn is_tray_window(hwnd: HWND) -> bool {
    let class = class_name(hwnd);
    class == PRIMARY_CLASS || class == SECONDARY_CLASS
}

fn appbar_data() -> APPBARDATA {
    APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        ..Default::default()
    }
}

/// Current taskbar state bits (`ABS_AUTOHIDE`, `ABS_ALWAYSONTOP`).
pub fn get_state() -> u32 {
    let mut data = appbar_data();
    (unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) }) as u32
}

/// Returns false when there is no taskbar to talk to (Explorer not running).
fn set_state(state: u32) -> bool {
    let Some(tray) = primary_tray() else {
        return false;
    };
    let mut data = appbar_data();
    data.hWnd = tray;
    data.lParam = LPARAM(state as isize);
    unsafe { SHAppBarMessage(ABM_SETSTATE, &mut data) };
    true
}

fn show_trays(show: bool) {
    let command = if show { SW_SHOWNOACTIVATE } else { SW_HIDE };
    for hwnd in tray_windows() {
        unsafe {
            let _ = ShowWindow(hwnd, command);
        }
    }
}

/// Put the taskbar into `mode`. The journal is on disk before the first change.
pub fn apply(mode: TaskbarMode) -> AppResult<()> {
    let tray = primary_tray()
        .ok_or_else(|| AppError::Other("the Windows taskbar window was not found".into()))?;

    let mut journal = journal::load().unwrap_or_else(|| TaskbarJournal {
        version: journal::JOURNAL_VERSION,
        pid: std::process::id(),
        original_state: get_state(),
        taskbar_was_visible: unsafe { IsWindowVisible(tray) }.as_bool(),
        applied_mode: mode,
    });
    journal.pid = std::process::id();
    journal.applied_mode = mode;
    journal::save(&journal)?;

    set_state(journal::managed_state(journal.original_state));
    match mode {
        TaskbarMode::AutoHide => {
            HIDE_ENFORCED.store(false, Ordering::SeqCst);
            if journal.taskbar_was_visible {
                show_trays(true);
            }
        }
        TaskbarMode::Hidden => {
            HIDE_ENFORCED.store(true, Ordering::SeqCst);
            show_trays(false);
        }
    }
    log::info!(
        "taskbar: applied {mode:?} (original state {:#x}, was visible: {})",
        journal.original_state,
        journal.taskbar_was_visible
    );
    Ok(())
}

/// Undo whatever the journal says Glassdock did. Returns true if a restore ran.
/// With no journal there is nothing to undo and nothing is touched.
pub fn restore_from_journal() -> bool {
    HIDE_ENFORCED.store(false, Ordering::SeqCst);
    let Some(journal) = journal::load() else {
        return false;
    };
    let plan = journal::restore_plan(&journal);
    if plan.show_taskbar {
        show_trays(true);
    }
    if set_state(plan.state) {
        journal::clear();
        true
    } else {
        // No taskbar right now (Explorer is restarting). Keep the journal so
        // the next start, or the TaskbarCreated handler, can finish the job.
        false
    }
}

/// Make the taskbar windows visible regardless of the journal. Used by
/// `--restore` as a last resort.
pub fn force_show() {
    HIDE_ENFORCED.store(false, Ordering::SeqCst);
    show_trays(true);
}

/// Called by the WinEvent hook when a window is shown. In "hide completely"
/// mode, hides the taskbar again if Explorer just showed it.
pub fn on_window_shown(hwnd: HWND) {
    if !HIDE_ENFORCED.load(Ordering::SeqCst) || !is_tray_window(hwnd) {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if REHIDE_WINDOW_START.swap(now, Ordering::SeqCst) != now {
        REHIDE_COUNT.store(0, Ordering::SeqCst);
    }
    if REHIDE_COUNT.fetch_add(1, Ordering::SeqCst) >= MAX_REHIDES_PER_SECOND {
        log::warn!("taskbar: Explorer keeps showing the taskbar; pausing re-hide for this second");
        return;
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}
