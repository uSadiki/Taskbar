//! Decides whether a fullscreen app or game is in the foreground on the dock's
//! monitor, in which case the dock hides.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindowVisible};

use super::win::{class_name, window_rect};
use crate::geometry::Rect;

/// Shell surfaces that cover the whole monitor without being fullscreen apps:
/// the desktop, the taskbars, Task View and Alt-Tab, Start, Search and the
/// notification centre. The real taskbar stays up for all of them.
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "XamlExplorerHostIslandWindow",
    "MultitaskingViewFrame",
    "Windows.UI.Core.CoreWindow",
    "ForegroundStaging",
    "TaskSwitcherWnd",
    "TaskSwitcherOverlayWnd",
    "LockScreenBackstopFrame",
];

pub fn is_shell_class(class: &str) -> bool {
    SHELL_CLASSES.contains(&class)
}

/// A window is fullscreen on a monitor when it covers every pixel of it.
/// A maximized window never does while the dock is registered as an appbar,
/// because it stops at the dock's reserved strip.
pub fn covers_monitor(window: Rect, monitor: Rect) -> bool {
    window.contains_rect(&monitor)
}

/// `Some(true)` if the foreground window is a fullscreen app on `monitor`,
/// `Some(false)` if it clearly is not, `None` when there is no foreground
/// window right now (mid-switch), in which case the caller keeps its state.
pub fn foreground_is_fullscreen(
    own: HWND,
    monitor: Rect,
    monitor_is_primary: bool,
) -> Option<bool> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_invalid() {
        return None;
    }
    if foreground == own {
        return Some(false);
    }
    if !unsafe { IsWindowVisible(foreground) }.as_bool() || is_shell_class(&class_name(foreground))
    {
        return Some(false);
    }
    if window_rect(foreground).is_some_and(|rect| covers_monitor(rect, monitor)) {
        return Some(true);
    }
    // Exclusive-mode Direct3D is reported for the primary monitor only.
    let exclusive = monitor_is_primary
        && unsafe { SHQueryUserNotificationState() }
            .is_ok_and(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN);
    Some(exclusive)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect::new(0, 0, 2560, 1600);

    #[test]
    fn exact_and_oversized_windows_cover_the_monitor() {
        assert!(covers_monitor(MONITOR, MONITOR));
        // Borderless games often extend a few pixels past every edge.
        assert!(covers_monitor(Rect::new(-8, -8, 2568, 1608), MONITOR));
    }

    #[test]
    fn maximized_window_above_the_dock_does_not_cover_it() {
        // Work area ends where the dock's 102px strip begins.
        assert!(!covers_monitor(Rect::new(0, 0, 2560, 1498), MONITOR));
        // Standard maximized overhang on the sides still stops at the dock.
        assert!(!covers_monitor(Rect::new(-11, -11, 2571, 1498), MONITOR));
    }

    #[test]
    fn window_on_another_monitor_does_not_count() {
        assert!(!covers_monitor(Rect::new(2560, 0, 4480, 1080), MONITOR));
    }

    #[test]
    fn shell_surfaces_are_recognised() {
        assert!(is_shell_class("Progman"));
        assert!(is_shell_class("XamlExplorerHostIslandWindow"));
        assert!(!is_shell_class("Chrome_WidgetWin_1"));
        assert!(!is_shell_class("ApplicationFrameWindow"));
    }
}
