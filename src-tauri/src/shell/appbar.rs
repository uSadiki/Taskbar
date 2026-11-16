//! AppBar registration: reserves the dock's strip of the screen so maximized
//! windows stop at it, the way they do for the taskbar.

use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_TOP, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
    ABM_WINDOWPOSCHANGED, APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::geometry::Rect;
use crate::settings::Edge;

/// Message Explorer uses to send `ABN_*` notifications to the dock window.
pub const CALLBACK_MSG: u32 = WM_APP + 0x60;

static REGISTERED: AtomicBool = AtomicBool::new(false);

fn data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        ..Default::default()
    }
}

const fn edge_code(edge: Edge) -> u32 {
    match edge {
        Edge::Top => ABE_TOP,
        Edge::Bottom => ABE_BOTTOM,
    }
}

pub fn is_registered() -> bool {
    REGISTERED.load(Ordering::SeqCst)
}

pub fn register(hwnd: HWND) -> bool {
    if is_registered() {
        return true;
    }
    let mut abd = data(hwnd);
    abd.uCallbackMessage = CALLBACK_MSG;
    let ok = unsafe { SHAppBarMessage(ABM_NEW, &mut abd) } != 0;
    REGISTERED.store(ok, Ordering::SeqCst);
    if !ok {
        log::error!("appbar: ABM_NEW failed; the dock will float without reserving space");
    }
    ok
}

pub fn unregister(hwnd: HWND) {
    if REGISTERED.swap(false, Ordering::SeqCst) {
        let mut abd = data(hwnd);
        unsafe { SHAppBarMessage(ABM_REMOVE, &mut abd) };
    }
}

/// Explorer restarted and forgot every appbar; forget the registration too so
/// `register` runs again.
pub fn forget_registration() {
    REGISTERED.store(false, Ordering::SeqCst);
}

/// Ask for a strip `thickness` pixels deep along `edge` of `monitor` and return
/// the strip the system granted. The system may push it inward if another
/// appbar or a non-hidden taskbar is on the same edge.
pub fn set_pos(hwnd: HWND, edge: Edge, monitor: Rect, thickness: i32) -> Rect {
    let mut abd = data(hwnd);
    abd.uEdge = edge_code(edge);
    abd.rc = proposed_rect(edge, monitor, thickness).into();
    unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut abd) };
    abd.rc = finalize_rect(edge, abd.rc.into(), thickness).into();
    unsafe { SHAppBarMessage(ABM_SETPOS, &mut abd) };
    abd.rc.into()
}

/// The dock must forward these two window messages so the shell keeps the
/// z-order of auto-hide appbars on the same edge correct.
pub fn activate(hwnd: HWND) {
    if is_registered() {
        let mut abd = data(hwnd);
        unsafe { SHAppBarMessage(ABM_ACTIVATE, &mut abd) };
    }
}

pub fn window_pos_changed(hwnd: HWND) {
    if is_registered() {
        let mut abd = data(hwnd);
        unsafe { SHAppBarMessage(ABM_WINDOWPOSCHANGED, &mut abd) };
    }
}

/// The strip to propose: the full width of the monitor at the chosen edge.
pub fn proposed_rect(edge: Edge, monitor: Rect, thickness: i32) -> Rect {
    match edge {
        Edge::Bottom => Rect::new(
            monitor.left,
            monitor.bottom - thickness,
            monitor.right,
            monitor.bottom,
        ),
        Edge::Top => Rect::new(
            monitor.left,
            monitor.top,
            monitor.right,
            monitor.top + thickness,
        ),
    }
}

/// `ABM_QUERYPOS` only subtracts occupied space; it doesn't keep the strip's
/// depth. Re-derive the depth from the edge the system left in place.
pub fn finalize_rect(edge: Edge, queried: Rect, thickness: i32) -> Rect {
    match edge {
        Edge::Bottom => Rect::new(
            queried.left,
            queried.bottom - thickness,
            queried.right,
            queried.bottom,
        ),
        Edge::Top => Rect::new(
            queried.left,
            queried.top,
            queried.right,
            queried.top + thickness,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect::new(0, 0, 2560, 1600);

    #[test]
    fn proposes_a_full_width_strip_at_the_edge() {
        assert_eq!(
            proposed_rect(Edge::Bottom, MONITOR, 102),
            Rect::new(0, 1498, 2560, 1600)
        );
        assert_eq!(
            proposed_rect(Edge::Top, MONITOR, 102),
            Rect::new(0, 0, 2560, 102)
        );
    }

    #[test]
    fn works_on_monitors_with_negative_origins() {
        let left_monitor = Rect::new(-1920, -200, 0, 880);
        assert_eq!(
            proposed_rect(Edge::Bottom, left_monitor, 68),
            Rect::new(-1920, 812, 0, 880)
        );
        assert_eq!(
            proposed_rect(Edge::Top, left_monitor, 68),
            Rect::new(-1920, -200, 0, -132)
        );
    }

    #[test]
    fn keeps_its_depth_when_the_system_pushes_it_inward() {
        // A visible 72px taskbar on the bottom edge: the query trims the strip
        // to 30px, and the dock sits directly above the taskbar at full depth.
        let queried = Rect::new(0, 1498, 2560, 1528);
        assert_eq!(
            finalize_rect(Edge::Bottom, queried, 102),
            Rect::new(0, 1426, 2560, 1528)
        );

        // Something occupies the top 40px.
        let queried = Rect::new(0, 40, 2560, 102);
        assert_eq!(
            finalize_rect(Edge::Top, queried, 102),
            Rect::new(0, 40, 2560, 142)
        );
    }

    #[test]
    fn unchanged_query_is_a_no_op() {
        let proposed = proposed_rect(Edge::Bottom, MONITOR, 102);
        assert_eq!(finalize_rect(Edge::Bottom, proposed, 102), proposed);
    }
}
