//! Whether an app window is in the dock's way, and the rules for when the dock
//! slides away and comes back.
//!
//! With "hide for windows" on, the dock reserves no screen space. Maximized
//! (and snapped, or simply large) windows then reach the screen edge, and the
//! dock gets out of their way: it slides off the edge while a window overlaps
//! it and comes back when the pointer is pushed against that edge.

use std::mem::size_of;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetWindowLongPtrW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};
use windows::core::BOOL;

use super::fullscreen::is_shell_class;
use super::win::{class_name, window_rect};
use crate::geometry::Rect;

/// Windows smaller than this (physical pixels, either side) are helpers,
/// badges or notifications, never what the dock should make room for.
const MIN_SIDE: i32 = 64;
/// Overlaps thinner than this are a window resting against the dock, not
/// covering it.
const MIN_OVERLAP: i32 = 2;

/// The window's visible bounds. `GetWindowRect` includes the invisible resize
/// border, which would make a window that ends just above the dock overlap it.
fn visible_bounds(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut rect as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        )
    }
    .is_ok();
    if ok {
        Some(rect.into())
    } else {
        window_rect(hwnd)
    }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            size_of::<u32>() as u32,
        )
    };
    result.is_ok() && cloaked != 0
}

/// True when `window` overlaps `dock` by more than a sliver.
pub fn overlaps(window: Rect, dock: Rect) -> bool {
    if window.width() < MIN_SIDE || window.height() < MIN_SIDE {
        return false;
    }
    window
        .intersect(&dock)
        .is_some_and(|r| r.width() >= MIN_OVERLAP && r.height() >= MIN_OVERLAP)
}

/// An app window that overlaps the dock: on screen, not minimized or cloaked
/// (other virtual desktops, suspended Store apps), not Glassdock's own, and
/// not a menu, tooltip, overlay or piece of the shell.
///
/// Cheapest tests first: most windows are ruled out by the window rectangle
/// (which includes the invisible border, so it is never smaller than the
/// visible bounds) before the slower DWM queries run.
fn overlapping_app_window(hwnd: HWND, dock: Rect, own_pid: u32) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TRANSPARENT.0) != 0 {
            return false;
        }
    }
    if !window_rect(hwnd).is_some_and(|r| overlaps(r, dock)) {
        return false;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid != own_pid
        && !is_shell_class(&class_name(hwnd))
        && !is_cloaked(hwnd)
        && visible_bounds(hwnd).is_some_and(|r| overlaps(r, dock))
}

struct Search {
    dock: Rect,
    own_pid: u32,
    found: bool,
}

unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the address of the Search in `window_overlaps`, which
    // outlives the EnumWindows call.
    let search = unsafe { &mut *(data.0 as *mut Search) };
    if overlapping_app_window(hwnd, search.dock, search.own_pid) {
        search.found = true;
        return BOOL(0); // stop enumerating
    }
    BOOL(1)
}

/// Whether any app window overlaps `dock` (screen coordinates, physical pixels).
pub fn window_overlaps(dock: Rect) -> bool {
    let mut search = Search {
        dock,
        own_pid: std::process::id(),
        found: false,
    };
    unsafe {
        // Returns an error when the callback stops early; `found` says why.
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

// ---------------------------------------------------------------------------
// When to show the dock
// ---------------------------------------------------------------------------

/// How long the pointer has to rest against the screen edge to bring the
/// dock up. Long enough that flinging the pointer past the edge doesn't.
pub const REVEAL_DWELL: Duration = Duration::from_millis(120);
/// How long the dock stays up after the pointer leaves it.
pub const HIDE_DELAY: Duration = Duration::from_millis(500);

/// What the dock sees on each check.
#[derive(Debug, Clone, Copy)]
pub struct Observed {
    /// An app window overlaps the dock.
    pub covered: bool,
    /// The pointer is pressed against the screen edge, level with the dock.
    pub at_edge: bool,
    /// The pointer is over the dock (or the gap between it and the edge).
    pub over_dock: bool,
    /// A popover, menu or alert is open above the dock.
    pub open: bool,
    pub now: Instant,
}

/// The dock's hide-for-windows state.
#[derive(Debug, Default, Clone)]
pub struct AutoHide {
    /// The dock is up over a covering window, brought there by the pointer
    /// or by an alert.
    revealed: bool,
    /// When the pointer arrived at the edge, while it stays there.
    edge_since: Option<Instant>,
    /// When the pointer left the revealed dock.
    left_at: Option<Instant>,
}

impl AutoHide {
    /// Fold in one observation; returns whether the dock should be on screen.
    pub fn step(&mut self, seen: Observed) -> bool {
        if !seen.covered {
            *self = AutoHide::default();
            return true;
        }
        if self.revealed {
            if seen.open || seen.over_dock || seen.at_edge {
                self.left_at = None;
                return true;
            }
            let left_at = *self.left_at.get_or_insert(seen.now);
            if seen.now.duration_since(left_at) < HIDE_DELAY {
                return true;
            }
            *self = AutoHide::default();
            return false;
        }
        // Covered and out of the way. An alert brings the dock up, and so
        // does the pointer resting against the edge.
        if seen.open {
            self.reveal();
            return true;
        }
        if seen.at_edge {
            let since = *self.edge_since.get_or_insert(seen.now);
            if seen.now.duration_since(since) >= REVEAL_DWELL {
                self.reveal();
                return true;
            }
        } else {
            self.edge_since = None;
        }
        false
    }

    fn reveal(&mut self) {
        self.revealed = true;
        self.edge_since = None;
        self.left_at = None;
    }

    #[cfg(test)]
    fn is_revealed(&self) -> bool {
        self.revealed
    }
}

/// The dock's offset toward the screen edge at time `t` of a slide that
/// takes `duration`: ease-out, so it starts quickly and settles gently.
pub fn slide_offset(from: i32, to: i32, elapsed: Duration, duration: Duration) -> i32 {
    if duration.is_zero() || elapsed >= duration {
        return to;
    }
    let t = elapsed.as_secs_f64() / duration.as_secs_f64();
    let eased = 1.0 - (1.0 - t).powi(3);
    from + ((f64::from(to - from)) * eased).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCK: Rect = Rect::new(400, 1502, 2160, 1592);

    #[test]
    fn maximized_and_snapped_windows_overlap_the_dock() {
        // Maximized with no reserved strip: visible bounds are the monitor.
        assert!(overlaps(Rect::new(0, 0, 2560, 1600), DOCK));
        // Snapped to the left half.
        assert!(overlaps(Rect::new(0, 0, 1280, 1600), DOCK));
    }

    #[test]
    fn windows_clear_of_the_dock_do_not() {
        // Ends exactly where the dock begins.
        assert!(!overlaps(Rect::new(0, 0, 2560, 1502), DOCK));
        // A corner window beside the pill.
        assert!(!overlaps(Rect::new(0, 1000, 390, 1600), DOCK));
        // Touching by a single pixel.
        assert!(!overlaps(Rect::new(0, 0, 2560, 1503), DOCK));
        // Small helper windows never count.
        assert!(!overlaps(Rect::new(1000, 1550, 1040, 1590), DOCK));
    }

    fn seen(now: Instant, covered: bool, at_edge: bool, over_dock: bool, open: bool) -> Observed {
        Observed {
            covered,
            at_edge,
            over_dock,
            open,
            now,
        }
    }

    #[test]
    fn shown_while_nothing_covers_it() {
        let mut state = AutoHide::default();
        let t = Instant::now();
        assert!(state.step(seen(t, false, false, false, false)));
        assert!(state.step(seen(t, false, true, true, false)));
    }

    #[test]
    fn hides_for_a_covering_window_and_returns_when_it_goes() {
        let mut state = AutoHide::default();
        let t = Instant::now();
        assert!(!state.step(seen(t, true, false, false, false)));
        assert!(state.step(seen(t, false, false, false, false)));
    }

    #[test]
    fn pointer_at_the_edge_reveals_after_a_short_rest() {
        let mut state = AutoHide::default();
        let t = Instant::now();
        assert!(!state.step(seen(t, true, true, false, false)));
        assert!(!state.step(seen(
            t + Duration::from_millis(60),
            true,
            true,
            false,
            false
        )));
        assert!(state.step(seen(t + REVEAL_DWELL, true, true, false, false)));
        assert!(state.is_revealed());
    }

    #[test]
    fn passing_the_edge_does_not_reveal() {
        let mut state = AutoHide::default();
        let t = Instant::now();
        assert!(!state.step(seen(t, true, true, false, false)));
        assert!(!state.step(seen(
            t + Duration::from_millis(50),
            true,
            false,
            false,
            false
        )));
        // The rest starts over when the pointer comes back.
        assert!(!state.step(seen(
            t + Duration::from_millis(200),
            true,
            true,
            false,
            false
        )));
    }

    #[test]
    fn revealed_dock_stays_while_used_and_leaves_after_the_pointer() {
        let mut state = AutoHide::default();
        let t = Instant::now();
        state.step(seen(t, true, true, false, false));
        assert!(state.step(seen(t + REVEAL_DWELL, true, true, false, false)));
        // On the dock, then a popover open with the pointer elsewhere.
        assert!(state.step(seen(t + Duration::from_secs(2), true, false, true, false)));
        assert!(state.step(seen(t + Duration::from_secs(5), true, false, false, true)));
        // Popover closed, pointer away: stays briefly, then goes.
        let gone = t + Duration::from_secs(6);
        assert!(state.step(seen(gone, true, false, false, false)));
        assert!(state.step(seen(gone + HIDE_DELAY / 2, true, false, false, false)));
        assert!(!state.step(seen(gone + HIDE_DELAY, true, false, false, false)));
        assert!(!state.is_revealed());
    }

    #[test]
    fn coming_back_during_the_delay_keeps_it_up() {
        let mut state = AutoHide::default();
        let t = Instant::now();
        state.step(seen(t, true, true, false, false));
        state.step(seen(t + REVEAL_DWELL, true, true, false, false));
        let away = t + Duration::from_secs(1);
        assert!(state.step(seen(away, true, false, false, false)));
        assert!(state.step(seen(away + HIDE_DELAY / 2, true, false, true, false)));
        assert!(state.step(seen(away + HIDE_DELAY * 2, true, false, true, false)));
    }

    #[test]
    fn an_alert_brings_the_dock_up() {
        let mut state = AutoHide::default();
        let t = Instant::now();
        assert!(!state.step(seen(t, true, false, false, false)));
        assert!(state.step(seen(t, true, false, false, true)));
        assert!(state.is_revealed());
    }

    #[test]
    fn slide_eases_out_and_lands_exactly() {
        let d = Duration::from_millis(160);
        assert_eq!(slide_offset(0, 120, Duration::ZERO, d), 0);
        let half = slide_offset(0, 120, d / 2, d);
        assert!(half > 60, "ease-out is past halfway at half time: {half}");
        assert_eq!(slide_offset(0, 120, d, d), 120);
        assert_eq!(slide_offset(120, 0, d * 2, d), 0);
    }
}
