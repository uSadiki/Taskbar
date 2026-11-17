//! The dock window at runtime: where it sits, which parts of it are visible
//! and clickable, and how it reacts to the shell.
//!
//! Everything here runs on the main thread (setup, the window proc, WinEvent
//! callbacks and synchronous commands all do). The state lock is never held
//! across a Win32 call that can re-enter the window proc.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, HGDIOBJ, HRGN, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
    HWND_TOPMOST, IsWindow, KillTimer, PostMessageW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_APP, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};
use windows::core::BOOL;

use super::cover::{self, AutoHide, Observed};
use super::monitors::{self, MonitorInfo};
use super::win::{hwnd_from_isize, hwnd_to_isize, window_rect};
use super::{appbar, fullscreen, hooks, taskbar};
use crate::error::{AppError, AppResult};
use crate::geometry::{Rect, RectF};
use crate::paths;
use crate::settings::{DockLayoutKind, DockSize, Edge, SettingsStore};

/// Gap between the dock and the screen edge, and between the dock and the
/// windows above it, in logical pixels.
pub const DOCK_GAP: f64 = 8.0;
/// Room above (or below) the dock for popovers, in logical pixels.
pub const POPOVER_HEADROOM: f64 = 640.0;
/// Extra clickable/visible margin around each reported region, for shadows.
const REGION_MARGIN: f64 = 12.0;

/// Posted to the dock window to run a (coalesced) reposition.
pub const WM_APP_REPOSITION: u32 = WM_APP + 0x61;

pub struct DockRuntime {
    pub app: AppHandle,
    /// Safe mode: no AppBar, no taskbar changes.
    pub safe: bool,
    hwnd: isize,
    state: Mutex<DockState>,
}

impl DockRuntime {
    pub fn hwnd(&self) -> HWND {
        hwnd_from_isize(self.hwnd)
    }
}

#[derive(Default)]
struct DockState {
    placement: Option<Placement>,
    /// Interactive rectangles last reported by the frontend, in CSS pixels.
    regions: Vec<RectF>,
    /// The user chose "Restore taskbar": leave the taskbar alone until Reload.
    taskbar_suspended: bool,
    fullscreen_hidden: bool,
    drag: Option<Drag>,
    /// The window that had focus before the dock took it for a menu.
    previous_foreground: isize,
    /// Hide for windows: when the dock gets out of the way.
    auto_hide: AutoHide,
    /// The last answer to "does a window overlap the dock?".
    covered: bool,
    /// Checks since the last overlap test, to space those tests out.
    ticks: u32,
    /// Interval of the hide-for-windows timer, in ms (0: not running).
    poll_ms: u32,
    /// How far the window is pushed past the screen edge, in physical pixels.
    offset: i32,
    slide: Option<Slide>,
    /// The window is hidden because it slid away for a window.
    slid_away: bool,
}

/// A slide of the dock toward or away from the screen edge.
#[derive(Debug, Clone, Copy)]
struct Slide {
    from: i32,
    to: i32,
    started: Instant,
}

/// Where the dock currently is, in physical pixels.
#[derive(Debug, Clone, PartialEq)]
struct Placement {
    edge: Edge,
    monitor: MonitorInfo,
    /// The reserved strip.
    band: Rect,
    /// The dock window: the strip plus popover headroom.
    window: Rect,
    scale: f64,
}

struct Drag {
    cursor_y: i32,
    window_top: i32,
}

/// What the frontend needs to lay the dock out, in logical pixels.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockLayout {
    pub edge: Edge,
    pub layout: DockLayoutKind,
    pub dock_height: f64,
    pub gap: f64,
    pub band_height: f64,
    pub window_width: f64,
    pub window_height: f64,
    pub scale: f64,
    pub monitor_id: String,
    pub safe_mode: bool,
    /// The window's top-left corner relative to the monitor's, in physical
    /// pixels. The glass uses it to find the wallpaper pixels behind it.
    pub origin_x: i32,
    pub origin_y: i32,
    /// Monitor size in physical pixels.
    pub monitor_width: i32,
    pub monitor_height: i32,
}

/// Sent while the dock is being dragged, so the glass can follow.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowOrigin {
    pub origin_x: i32,
    pub origin_y: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub dev_build: bool,
    pub safe_mode: bool,
    pub taskbar_suspended: bool,
    pub log_dir: String,
    pub settings_path: String,
}

/// Timer id for re-checking fullscreen state after an inconclusive check.
pub const FULLSCREEN_RETRY_TIMER: usize = 0x4653;
/// Timer id for re-checking while hidden for a fullscreen app.
pub const FULLSCREEN_RECHECK_TIMER: usize = 0x4654;
static FULLSCREEN_RETRIES: AtomicU32 = AtomicU32::new(0);
/// Timer id for the hide-for-windows checks.
pub const AUTO_HIDE_TIMER: usize = 0x4655;
/// Timer id for the steps of a slide.
pub const SLIDE_TIMER: usize = 0x4656;
/// Timer id for a check shortly after windows were rearranged.
pub const COVER_CHECK_TIMER: usize = 0x4657;

/// Hide-for-windows check intervals. Quick while a window covers the dock
/// (the pointer has to be noticed at the edge), slow otherwise (only a
/// window arriving over the dock matters), slower still behind a
/// fullscreen app.
const POLL_COVERED_MS: u32 = 40;
const POLL_CLEAR_MS: u32 = 250;
const POLL_FULLSCREEN_MS: u32 = 1000;
/// While covered, look for overlapping windows only every this many checks.
const COVER_TEST_EVERY: u32 = 6;
const SLIDE_OUT: Duration = Duration::from_millis(180);
const SLIDE_IN: Duration = Duration::from_millis(160);
const SLIDE_STEP_MS: u32 = 10;
/// Room around the dock that still counts as being on it, in logical pixels.
const HOVER_MARGIN: f64 = 10.0;

static RUNTIME: OnceLock<DockRuntime> = OnceLock::new();
static REPOSITION_PENDING: AtomicBool = AtomicBool::new(false);
static REPOSITIONING: AtomicBool = AtomicBool::new(false);

pub fn init(app: AppHandle, hwnd: HWND, safe: bool) {
    let _ = RUNTIME.set(DockRuntime {
        app,
        safe,
        hwnd: hwnd_to_isize(hwnd),
        state: Mutex::new(DockState::default()),
    });
}

pub fn runtime() -> Option<&'static DockRuntime> {
    RUNTIME.get()
}

// ---------------------------------------------------------------------------
// Pure placement math
// ---------------------------------------------------------------------------

/// Gap around the dock for a layout: the pill floats, the full-width bar is
/// flush with the screen edge.
pub const fn gap_for(layout: DockLayoutKind) -> f64 {
    match layout {
        DockLayoutKind::Pill => DOCK_GAP,
        DockLayoutKind::Full => 0.0,
    }
}

/// Depth of the reserved strip in physical pixels: the dock plus its gap on
/// each side.
pub fn band_thickness_px(size: DockSize, layout: DockLayoutKind, scale: f64) -> i32 {
    ((size.dock_height() + 2.0 * gap_for(layout)) * scale).round() as i32
}

/// Popover headroom in physical pixels, never more than the monitor has left.
pub fn headroom_px(scale: f64, monitor_height: i32, band_thickness: i32) -> i32 {
    let wanted = (POPOVER_HEADROOM * scale).round() as i32;
    wanted.min(monitor_height - band_thickness).max(0)
}

/// Safe mode has no reserved strip, so the dock sits at the edge of the work
/// area, clear of the taskbar.
pub fn band_in_work_area(edge: Edge, work: Rect, thickness: i32) -> Rect {
    match edge {
        Edge::Bottom => Rect::new(work.left, work.bottom - thickness, work.right, work.bottom),
        Edge::Top => Rect::new(work.left, work.top, work.right, work.top + thickness),
    }
}

/// The dock window: the strip, extended toward the middle of the screen.
pub fn window_rect_for_band(edge: Edge, band: Rect, headroom: i32) -> Rect {
    match edge {
        Edge::Bottom => Rect::new(band.left, band.top - headroom, band.right, band.bottom),
        Edge::Top => Rect::new(band.left, band.top, band.right, band.bottom + headroom),
    }
}

/// The strip in window coordinates.
pub fn band_in_window(edge: Edge, window: Rect, band: Rect) -> Rect {
    let _ = edge;
    band.offset(-window.left, -window.top)
}

/// Which edge a dragged dock snaps to: the half of the monitor it was dropped in.
pub fn snap_edge(cursor_y: i32, monitor: Rect) -> Edge {
    if cursor_y < monitor.top + monitor.height() / 2 {
        Edge::Top
    } else {
        Edge::Bottom
    }
}

// ---------------------------------------------------------------------------
// Window setup
// ---------------------------------------------------------------------------

/// Tool window (no Alt-Tab entry, no taskbar button) that does not take focus
/// when clicked, so clicking the dock leaves the active app active.
pub fn apply_window_styles(hwnd: HWND) {
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let wanted = (current | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) & !WS_EX_APPWINDOW.0;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted as isize);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

/// Make clicks on the page not activate the dock.
///
/// `WS_EX_NOACTIVATE` on the dock window is not enough: the click lands on
/// WebView2's own child windows, and Chromium answers `WM_MOUSEACTIVATE` there
/// with "activate" unless the child's parent carries the no-activate style.
/// The children are created after the dock window, so this is re-applied
/// whenever the frontend reports its layout. It is idempotent.
pub fn mark_children_no_activate() {
    unsafe extern "system" fn each(child: HWND, _: LPARAM) -> BOOL {
        unsafe {
            let current = GetWindowLongPtrW(child, GWL_EXSTYLE) as u32;
            if current & WS_EX_NOACTIVATE.0 == 0 {
                SetWindowLongPtrW(child, GWL_EXSTYLE, (current | WS_EX_NOACTIVATE.0) as isize);
            }
        }
        BOOL(1)
    }
    let Some(rt) = runtime() else { return };
    unsafe {
        let _ = EnumChildWindows(Some(rt.hwnd()), Some(each), LPARAM(0));
    }
}

pub fn show() {
    let Some(rt) = runtime() else { return };
    mark_children_no_activate();
    // Stay hidden while a fullscreen app is in front, or while slid away for
    // a window; those checks show the dock again when it's time.
    {
        let state = rt.state.lock();
        if state.fullscreen_hidden || state.slid_away {
            return;
        }
    }
    unsafe {
        let _ = ShowWindow(rt.hwnd(), SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            rt.hwnd(),
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

// ---------------------------------------------------------------------------
// Positioning
// ---------------------------------------------------------------------------

/// Ask for a reposition on the next turn of the message loop. Requests made
/// before that turn collapse into one.
pub fn request_reposition() {
    let Some(rt) = runtime() else { return };
    if !REPOSITION_PENDING.swap(true, Ordering::SeqCst) {
        let posted =
            unsafe { PostMessageW(Some(rt.hwnd()), WM_APP_REPOSITION, WPARAM(0), LPARAM(0)) };
        if posted.is_err() {
            REPOSITION_PENDING.store(false, Ordering::SeqCst);
        }
    }
}

pub fn on_reposition_message() {
    REPOSITION_PENDING.store(false, Ordering::SeqCst);
    reposition();
}

/// Recompute the reserved strip and the window rectangle from the current
/// settings and monitor layout, and move the window there.
pub fn reposition() {
    let Some(rt) = runtime() else { return };
    if REPOSITIONING.swap(true, Ordering::SeqCst) {
        // Re-entered through a message sent during our own SetWindowPos or
        // SHAppBarMessage. Run again once the current pass is done.
        request_reposition();
        return;
    }
    if !reposition_budget_left() {
        REPOSITIONING.store(false, Ordering::SeqCst);
        return;
    }
    let result = reposition_inner(rt);
    REPOSITIONING.store(false, Ordering::SeqCst);
    match result {
        Ok(()) => {
            emit_layout();
            // The dock may be on a different monitor now, or the monitor may
            // have changed size. Cheap when nothing changed.
            crate::glass::request_refresh(&rt.app);
            // A fullscreen app may already be in front (at startup, or on
            // the monitor the dock just moved to).
            evaluate_fullscreen(None);
            // Likewise a window may already be in the dock's way.
            restart_auto_hide();
        }
        Err(e) => log::error!("dock: reposition failed: {e}"),
    }
}

/// Repositioning answers shell notifications, and repositioning can cause
/// shell notifications. If that ever turns into a feedback loop, stop feeding
/// it: allow a burst, then drop requests for the rest of the second.
fn reposition_budget_left() -> bool {
    const MAX_PER_SECOND: u32 = 30;
    static SECOND: AtomicU64 = AtomicU64::new(0);
    static COUNT: AtomicU32 = AtomicU32::new(0);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if SECOND.swap(now, Ordering::SeqCst) != now {
        COUNT.store(0, Ordering::SeqCst);
    }
    let used = COUNT.fetch_add(1, Ordering::SeqCst);
    if used == MAX_PER_SECOND {
        log::warn!("dock: more than {MAX_PER_SECOND} repositions in one second; pausing");
    }
    used < MAX_PER_SECOND
}

fn reposition_inner(rt: &DockRuntime) -> AppResult<()> {
    if rt.state.lock().drag.is_some() {
        return Ok(());
    }
    let settings = rt.app.state::<SettingsStore>().get();
    HIDE_FOR_WINDOWS.store(settings.general.hide_for_windows, Ordering::SeqCst);
    let all = monitors::enumerate();
    let monitor = monitors::pick(&all, settings.general.monitor.as_deref())
        .cloned()
        .ok_or_else(|| AppError::Other("no monitors found".into()))?;

    let scale = f64::from(monitor.dpi) / 96.0;
    let edge = settings.general.position;
    let thickness = band_thickness_px(
        settings.appearance.dock_size,
        settings.general.layout,
        scale,
    );
    let hwnd = rt.hwnd();

    let band = if !rt.safe && appbar::is_registered() {
        let reserve = !settings.general.hide_for_windows;
        appbar::set_pos(hwnd, edge, monitor.rect, thickness, reserve)
    } else {
        band_in_work_area(edge, monitor.work, thickness)
    };
    let window = window_rect_for_band(
        edge,
        band,
        headroom_px(scale, monitor.rect.height(), thickness),
    );

    // Keep a dock that slid away (or is sliding) where it is along its path.
    let offset = rt.state.lock().offset;
    let top = window.top + edge_direction(edge) * offset;
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            window.left,
            top,
            window.width(),
            window.height(),
            SWP_NOACTIVATE,
        )?;
    }

    let placement = Placement {
        edge,
        monitor,
        band,
        window,
        scale,
    };
    let changed = {
        let mut state = rt.state.lock();
        let changed = state.placement.as_ref() != Some(&placement);
        state.placement = Some(placement.clone());
        changed
    };
    if changed {
        log::info!(
            "dock: {edge:?} on {} at {scale:.2}x, strip {band:?}, window {window:?}",
            placement.monitor.id
        );
    }
    apply_region(rt);
    Ok(())
}

/// Limit the window to the union of the interactive rectangles. Outside that
/// region nothing is drawn and clicks go to whatever is behind the window.
fn apply_region(rt: &DockRuntime) {
    let (placement, regions) = {
        let state = rt.state.lock();
        (state.placement.clone(), state.regions.clone())
    };
    let Some(placement) = placement else { return };

    let bounds = Rect::new(0, 0, placement.window.width(), placement.window.height());
    let mut rects: Vec<Rect> = regions
        .iter()
        .filter_map(|r| {
            r.to_physical(placement.scale, REGION_MARGIN)
                .intersect(&bounds)
        })
        .collect();
    if rects.is_empty() {
        // Until the frontend reports its layout, expose the reserved strip only.
        rects.push(band_in_window(
            placement.edge,
            placement.window,
            placement.band,
        ));
    }

    unsafe {
        let mut union: Option<HRGN> = None;
        for rect in rects {
            let region = CreateRectRgn(rect.left, rect.top, rect.right, rect.bottom);
            if region.is_invalid() {
                continue;
            }
            match union {
                None => union = Some(region),
                Some(existing) => {
                    CombineRgn(Some(existing), Some(existing), Some(region), RGN_OR);
                    let _ = DeleteObject(HGDIOBJ(region.0));
                }
            }
        }
        if let Some(region) = union {
            // On success the system owns the region; on failure it is still ours.
            if SetWindowRgn(rt.hwnd(), Some(region), true) == 0 {
                let _ = DeleteObject(HGDIOBJ(region.0));
                log::warn!("dock: SetWindowRgn failed");
            }
        }
    }
}

pub fn set_regions(regions: Vec<RectF>) {
    let Some(rt) = runtime() else { return };
    rt.state.lock().regions = regions;
    apply_region(rt);
    // The page has rendered by now, so WebView2's child windows exist.
    mark_children_no_activate();
}

pub fn layout() -> Option<DockLayout> {
    let rt = runtime()?;
    let placement = rt.state.lock().placement.clone()?;
    let settings = rt.app.state::<SettingsStore>().get();
    Some(DockLayout {
        edge: placement.edge,
        layout: settings.general.layout,
        dock_height: settings.appearance.dock_size.dock_height(),
        gap: gap_for(settings.general.layout),
        band_height: f64::from(placement.band.height()) / placement.scale,
        window_width: f64::from(placement.window.width()) / placement.scale,
        window_height: f64::from(placement.window.height()) / placement.scale,
        scale: placement.scale,
        safe_mode: rt.safe,
        origin_x: placement.window.left - placement.monitor.rect.left,
        origin_y: placement.window.top - placement.monitor.rect.top,
        monitor_width: placement.monitor.rect.width(),
        monitor_height: placement.monitor.rect.height(),
        monitor_id: placement.monitor.id,
    })
}

/// The rectangle of the monitor the dock is on, once it has been placed.
pub fn monitor_rect() -> Option<Rect> {
    let rt = runtime()?;
    let state = rt.state.lock();
    state.placement.as_ref().map(|p| p.monitor.rect)
}

pub fn emit_layout() {
    if let (Some(rt), Some(layout)) = (runtime(), layout()) {
        let _ = rt.app.emit("dock://layout", layout);
    }
}

pub fn app_info() -> AppInfo {
    let (safe_mode, taskbar_suspended) = runtime()
        .map(|rt| (rt.safe, rt.state.lock().taskbar_suspended))
        .unwrap_or((false, false));
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        dev_build: cfg!(debug_assertions),
        safe_mode,
        taskbar_suspended,
        log_dir: paths::log_dir().display().to_string(),
        settings_path: paths::settings_path().display().to_string(),
    }
}

fn emit_app_info() {
    if let Some(rt) = runtime() {
        let _ = rt.app.emit("app://info", app_info());
    }
}

// ---------------------------------------------------------------------------
// Fullscreen apps
// ---------------------------------------------------------------------------

/// Hide the dock while a fullscreen app is in the foreground on its monitor,
/// and bring it back afterwards. Called on `ABN_FULLSCREENAPP` and on every
/// foreground change.
pub fn evaluate_fullscreen(candidate: Option<HWND>) {
    let Some(rt) = runtime() else { return };
    if rt.safe {
        return;
    }
    let Some((monitor_rect, primary)) = rt
        .state
        .lock()
        .placement
        .as_ref()
        .map(|p| (p.monitor.rect, p.monitor.primary))
    else {
        return;
    };
    let Some(fullscreen) =
        fullscreen::foreground_is_fullscreen(rt.hwnd(), candidate, monitor_rect, primary)
    else {
        // No foreground window at this instant. Ask again shortly; a few
        // attempts are enough to get past a window switch.
        if FULLSCREEN_RETRIES.fetch_add(1, Ordering::SeqCst) < 5 {
            unsafe {
                SetTimer(Some(rt.hwnd()), FULLSCREEN_RETRY_TIMER, 250, None);
            }
        }
        return;
    };
    FULLSCREEN_RETRIES.store(0, Ordering::SeqCst);

    let changed = {
        let mut state = rt.state.lock();
        let changed = state.fullscreen_hidden != fullscreen;
        state.fullscreen_hidden = fullscreen;
        changed
    };
    if !changed {
        return;
    }
    log::info!("dock: fullscreen app in foreground: {fullscreen}");
    if fullscreen {
        set_rendering(rt, false);
        unsafe {
            let _ = ShowWindow(rt.hwnd(), SW_HIDE);
            // Leaving fullscreen often doesn't change the foreground window
            // (Esc in a browser or video player), so check again now and
            // then while hidden. Nothing runs while the dock is visible.
            SetTimer(Some(rt.hwnd()), FULLSCREEN_RECHECK_TIMER, 1500, None);
        }
    } else {
        unsafe {
            let _ = KillTimer(Some(rt.hwnd()), FULLSCREEN_RECHECK_TIMER);
        }
        // Back from fullscreen into an app that fills the screen: stay out
        // of its way; the hide-for-windows check brings the dock back later.
        if rt.state.lock().slid_away {
            return;
        }
        set_rendering(rt, true);
        show();
    }
}

/// Tell WebView2 whether the page is on screen. Hiding the window alone
/// doesn't stop the page from rendering; this does, so a dock hidden for a
/// fullscreen game or video costs nothing.
fn set_rendering(rt: &DockRuntime, visible: bool) {
    let Some(window) = rt.app.get_webview_window("dock") else {
        return;
    };
    let result = window.with_webview(move |webview| unsafe {
        if let Err(e) = webview.controller().SetIsVisible(visible) {
            log::warn!("dock: could not change page visibility: {e}");
        }
    });
    if let Err(e) = result {
        log::warn!("dock: could not reach the page: {e}");
    }
}

/// The re-check timer set while the dock is hidden for a fullscreen app.
pub fn on_fullscreen_recheck() {
    evaluate_fullscreen(None);
}

pub fn on_foreground_changed(foreground: HWND) {
    let Some(rt) = runtime() else { return };
    if foreground == rt.hwnd() {
        log::debug!("dock: the dock window became the foreground window");
        return;
    }
    // Another window came to the front. If a popover is open it should
    // close, even when Windows did not let the dock take focus for it (in
    // which case no deactivation message will ever arrive).
    let _ = rt.app.emit("dock://blur", ());
    evaluate_fullscreen(Some(foreground));
}

/// The retry timer set by `evaluate_fullscreen` fired.
pub fn on_fullscreen_retry() {
    if let Some(rt) = runtime() {
        unsafe {
            let _ = KillTimer(Some(rt.hwnd()), FULLSCREEN_RETRY_TIMER);
        }
    }
    evaluate_fullscreen(None);
}

// ---------------------------------------------------------------------------
// Hide for windows
// ---------------------------------------------------------------------------

/// Which way the window moves to slide off its screen edge: down for the
/// bottom edge, up for the top.
const fn edge_direction(edge: Edge) -> i32 {
    match edge {
        Edge::Bottom => 1,
        Edge::Top => -1,
    }
}

/// The hide-for-windows setting as of the last reposition (every change to
/// it repositions), so the frequent checks don't copy the settings.
static HIDE_FOR_WINDOWS: AtomicBool = AtomicBool::new(false);

fn hide_for_windows(rt: &DockRuntime) -> bool {
    !rt.safe && HIDE_FOR_WINDOWS.load(Ordering::SeqCst)
}

/// Sorts the reported regions (physical pixels, window coordinates) into the
/// dock bar, which lies within the strip, and anything open above it: a
/// popover, a menu or an alert. Returns the bar, if reported, and whether
/// anything is open.
pub fn split_regions(band: Rect, regions: &[Rect]) -> (Option<Rect>, bool) {
    let slack = Rect::new(band.left - 2, band.top - 2, band.right + 2, band.bottom + 2);
    let mut bar: Option<Rect> = None;
    let mut open = false;
    for r in regions {
        if slack.contains_rect(r) {
            bar = Some(match bar {
                Some(b) => Rect::new(
                    b.left.min(r.left),
                    b.top.min(r.top),
                    b.right.max(r.right),
                    b.bottom.max(r.bottom),
                ),
                None => *r,
            });
        } else {
            open = true;
        }
    }
    (bar, open)
}

/// The area that counts as "on the dock": the bar, the gap between it and
/// the screen edge, and a margin around both.
pub fn hover_rect(edge: Edge, bar: Rect, monitor: Rect, margin: i32) -> Rect {
    match edge {
        Edge::Bottom => Rect::new(
            bar.left - margin,
            bar.top - margin,
            bar.right + margin,
            monitor.bottom,
        ),
        Edge::Top => Rect::new(
            bar.left - margin,
            monitor.top,
            bar.right + margin,
            bar.bottom + margin,
        ),
    }
}

/// The pointer is pressed against the dock's screen edge, level with the bar.
pub fn at_edge(edge: Edge, (x, y): (i32, i32), bar: Rect, monitor: Rect) -> bool {
    // The pointer stops at the last row of pixels.
    let on_edge = match edge {
        Edge::Bottom => y == monitor.bottom - 1,
        Edge::Top => y == monitor.top,
    };
    on_edge && x >= bar.left && x < bar.right
}

fn contains(rect: Rect, (x, y): (i32, i32)) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

/// How far the window has to move for the bar to be fully off screen.
pub fn travel_px(edge: Edge, bar: Rect, monitor: Rect) -> i32 {
    match edge {
        Edge::Bottom => monitor.bottom - bar.top + 1,
        Edge::Top => bar.bottom - monitor.top + 1,
    }
    .max(0)
}

fn cursor_pos() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some((point.x, point.y))
}

/// Start (or restart) the hide-for-windows checks, or stop them and put the
/// dock back in place if the option is off.
pub fn restart_auto_hide() {
    let Some(rt) = runtime() else { return };
    auto_hide_tick(rt, true);
}

pub fn on_auto_hide_timer() {
    if let Some(rt) = runtime() {
        auto_hide_tick(rt, false);
    }
}

/// Windows were rearranged (one came to the front, was minimized or
/// restored, or was moved): check for overlaps soon, without waiting for
/// the next regular check.
pub fn on_windows_rearranged() {
    let Some(rt) = runtime() else { return };
    if rt.state.lock().poll_ms != 0 {
        unsafe {
            SetTimer(Some(rt.hwnd()), COVER_CHECK_TIMER, 30, None);
        }
    }
}

pub fn on_cover_check_timer() {
    let Some(rt) = runtime() else { return };
    unsafe {
        let _ = KillTimer(Some(rt.hwnd()), COVER_CHECK_TIMER);
    }
    auto_hide_tick(rt, true);
}

fn set_poll(rt: &DockRuntime, ms: u32) {
    {
        let mut state = rt.state.lock();
        if state.poll_ms == ms {
            return;
        }
        state.poll_ms = ms;
    }
    unsafe {
        SetTimer(Some(rt.hwnd()), AUTO_HIDE_TIMER, ms, None);
    }
}

/// The option was turned off (or safe mode): no more checks, and the dock
/// goes back in place.
fn stop_auto_hide(rt: &DockRuntime) {
    let (placement, was_away, fullscreen) = {
        let mut state = rt.state.lock();
        if state.poll_ms == 0 && state.offset == 0 && !state.slid_away && state.slide.is_none() {
            return;
        }
        let was_away = state.slid_away || state.offset != 0 || state.slide.is_some();
        state.auto_hide = AutoHide::default();
        state.covered = false;
        state.poll_ms = 0;
        state.slide = None;
        state.offset = 0;
        state.slid_away = false;
        (state.placement.clone(), was_away, state.fullscreen_hidden)
    };
    unsafe {
        let _ = KillTimer(Some(rt.hwnd()), AUTO_HIDE_TIMER);
        let _ = KillTimer(Some(rt.hwnd()), SLIDE_TIMER);
        let _ = KillTimer(Some(rt.hwnd()), COVER_CHECK_TIMER);
    }
    if was_away {
        if let Some(placement) = placement {
            move_to_offset(rt, &placement, 0);
        }
        if !fullscreen {
            set_rendering(rt, true);
            show();
        }
    }
    log::info!("dock: hide for windows off");
}

fn auto_hide_tick(rt: &DockRuntime, test_now: bool) {
    if !hide_for_windows(rt) {
        stop_auto_hide(rt);
        return;
    }
    let (placement, regions, fullscreen, dragging, covered_before, ticks) = {
        let mut state = rt.state.lock();
        state.ticks = state.ticks.wrapping_add(1);
        (
            state.placement.clone(),
            state.regions.clone(),
            state.fullscreen_hidden,
            state.drag.is_some(),
            state.covered,
            state.ticks,
        )
    };
    let Some(placement) = placement else { return };
    if fullscreen {
        // Nothing to decide until the fullscreen app is gone.
        set_poll(rt, POLL_FULLSCREEN_MS);
        return;
    }
    if dragging {
        return;
    }

    let band = band_in_window(placement.edge, placement.window, placement.band);
    let physical: Vec<Rect> = regions
        .iter()
        .map(|r| r.to_physical(placement.scale, 0.0))
        .collect();
    let (bar, open) = split_regions(band, &physical);
    let bar = bar
        .unwrap_or(band)
        .offset(placement.window.left, placement.window.top);
    let monitor = placement.monitor.rect;

    let covered = if test_now || !covered_before || ticks % COVER_TEST_EVERY == 0 {
        cover::window_overlaps(bar)
    } else {
        covered_before
    };
    let cursor = cursor_pos();
    let margin = (HOVER_MARGIN * placement.scale).round() as i32;
    let seen = Observed {
        covered,
        at_edge: cursor.is_some_and(|c| at_edge(placement.edge, c, bar, monitor)),
        over_dock: cursor
            .is_some_and(|c| contains(hover_rect(placement.edge, bar, monitor, margin), c)),
        open,
        now: Instant::now(),
    };
    let visible = {
        let mut state = rt.state.lock();
        if state.covered != covered {
            log::debug!(
                "dock: a window {} the dock",
                if covered {
                    "covers"
                } else {
                    "no longer covers"
                }
            );
        }
        state.covered = covered;
        state.auto_hide.step(seen)
    };
    slide_to(
        rt,
        if visible {
            0
        } else {
            travel_px(placement.edge, bar, monitor)
        },
    );
    set_poll(
        rt,
        if covered {
            POLL_COVERED_MS
        } else {
            POLL_CLEAR_MS
        },
    );
}

/// Start sliding toward `target` (0: in place), unless already there or on
/// the way.
fn slide_to(rt: &DockRuntime, target: i32) {
    let (coming_back, fullscreen) = {
        let mut state = rt.state.lock();
        let heading = state.slide.map_or(state.offset, |s| s.to);
        if heading == target {
            return;
        }
        state.slide = Some(Slide {
            from: state.offset,
            to: target,
            started: Instant::now(),
        });
        let coming_back = state.slid_away && target < state.offset;
        if coming_back {
            state.slid_away = false;
        }
        (coming_back, state.fullscreen_hidden)
    };
    log::debug!("dock: sliding {}", if target == 0 { "in" } else { "away" });
    if coming_back && !fullscreen {
        set_rendering(rt, true);
        show();
    }
    unsafe {
        SetTimer(Some(rt.hwnd()), SLIDE_TIMER, SLIDE_STEP_MS, None);
    }
}

pub fn on_slide_timer() {
    let Some(rt) = runtime() else { return };
    let current = {
        let state = rt.state.lock();
        state.placement.clone().zip(state.slide)
    };
    let Some((placement, slide)) = current else {
        unsafe {
            let _ = KillTimer(Some(rt.hwnd()), SLIDE_TIMER);
        }
        return;
    };
    let duration = if slide.to > slide.from {
        SLIDE_OUT
    } else {
        SLIDE_IN
    };
    let elapsed = slide.started.elapsed();
    let offset = cover::slide_offset(slide.from, slide.to, elapsed, duration);
    move_to_offset(rt, &placement, offset);

    let done = elapsed >= duration;
    let hide = {
        let mut state = rt.state.lock();
        state.offset = offset;
        if done {
            state.slide = None;
            state.slid_away = offset != 0;
        }
        done && offset != 0
    };
    if done {
        unsafe {
            let _ = KillTimer(Some(rt.hwnd()), SLIDE_TIMER);
        }
    }
    if hide {
        // Off screen: stop drawing too, so a dock that is out of the way
        // costs nothing.
        set_rendering(rt, false);
        unsafe {
            let _ = ShowWindow(rt.hwnd(), SW_HIDE);
        }
    }
}

fn move_to_offset(rt: &DockRuntime, placement: &Placement, offset: i32) {
    let top = placement.window.top + edge_direction(placement.edge) * offset;
    unsafe {
        let _ = SetWindowPos(
            rt.hwnd(),
            None,
            placement.window.left,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

// ---------------------------------------------------------------------------
// Dragging between the top and bottom edge
// ---------------------------------------------------------------------------

fn cursor_y() -> Option<i32> {
    cursor_pos().map(|(_, y)| y)
}

pub fn drag_start() {
    let Some(rt) = runtime() else { return };
    let (Some(cursor), Some(window)) = (cursor_y(), window_rect(rt.hwnd())) else {
        return;
    };
    rt.state.lock().drag = Some(Drag {
        cursor_y: cursor,
        window_top: window.top,
    });
}

pub fn drag_move() {
    let Some(rt) = runtime() else { return };
    let Some(cursor) = cursor_y() else { return };
    let target = {
        let state = rt.state.lock();
        match (&state.drag, &state.placement) {
            (Some(drag), Some(placement)) => Some((
                placement.window.left,
                drag.window_top + (cursor - drag.cursor_y),
                placement.monitor.rect,
            )),
            _ => None,
        }
    };
    if let Some((left, top, monitor)) = target {
        unsafe {
            let _ = SetWindowPos(
                rt.hwnd(),
                None,
                left,
                top,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let origin = WindowOrigin {
            origin_x: left - monitor.left,
            origin_y: top - monitor.top,
        };
        let _ = rt.app.emit("dock://origin", origin);
    }
}

/// Ends a drag and returns the edge the dock was dropped on, or `None` if no
/// drag was in progress.
pub fn drag_end() -> Option<Edge> {
    let rt = runtime()?;
    let (drag, monitor) = {
        let mut state = rt.state.lock();
        (
            state.drag.take(),
            state.placement.as_ref().map(|p| p.monitor.rect),
        )
    };
    drag?;
    Some(snap_edge(cursor_y()?, monitor?))
}

// ---------------------------------------------------------------------------
// Focus, for menus and popovers
// ---------------------------------------------------------------------------

/// Take keyboard focus (the dock normally never does) so Esc and clicking
/// elsewhere can dismiss an open menu.
pub fn focus() {
    let Some(rt) = runtime() else { return };
    let foreground = unsafe { GetForegroundWindow() };
    if foreground != rt.hwnd() {
        rt.state.lock().previous_foreground = hwnd_to_isize(foreground);
    }
    log::debug!("dock: taking focus for a menu");
    if let Some(window) = rt.app.get_webview_window("dock") {
        let _ = window.set_focus();
    }
    unsafe {
        let _ = SetForegroundWindow(rt.hwnd());
    }
}

/// Give focus back to the window that had it before `focus`.
pub fn release_focus() {
    let Some(rt) = runtime() else { return };
    let previous = std::mem::take(&mut rt.state.lock().previous_foreground);
    if previous == 0 {
        return;
    }
    let previous = hwnd_from_isize(previous);
    unsafe {
        if GetForegroundWindow() == rt.hwnd() && IsWindow(Some(previous)).as_bool() {
            let _ = SetForegroundWindow(previous);
        }
    }
}

pub fn on_deactivated() {
    if let Some(rt) = runtime() {
        log::debug!("dock: lost activation");
        let _ = rt.app.emit("dock://blur", ());
    }
}

// ---------------------------------------------------------------------------
// Taskbar lifecycle
// ---------------------------------------------------------------------------

/// "Restore taskbar": put the taskbar back now and leave it alone until the
/// user reloads or changes the taskbar mode.
pub fn suspend_taskbar() {
    let Some(rt) = runtime() else { return };
    if rt.safe {
        return;
    }
    let restored = taskbar::restore_from_journal();
    rt.state.lock().taskbar_suspended = true;
    log::info!("taskbar: restored on request (journal applied: {restored}); management paused");
    request_reposition();
    emit_app_info();
}

/// Apply the configured taskbar mode, unless safe mode or a manual restore
/// says to keep hands off.
pub fn apply_taskbar_mode() {
    let Some(rt) = runtime() else { return };
    if rt.safe || rt.state.lock().taskbar_suspended {
        return;
    }
    let mode = rt.app.state::<SettingsStore>().get().general.taskbar_mode;
    if let Err(e) = taskbar::apply(mode) {
        log::error!("taskbar: could not apply {mode:?}: {e}");
    }
}

/// The user picked a taskbar mode in Settings: that ends a manual restore.
pub fn resume_taskbar() {
    if let Some(rt) = runtime() {
        rt.state.lock().taskbar_suspended = false;
    }
    apply_taskbar_mode();
    emit_app_info();
}

/// Explorer restarted: its taskbar is new and it has forgotten every appbar.
pub fn on_taskbar_created() {
    let Some(rt) = runtime() else { return };
    log::info!("shell: TaskbarCreated received; re-registering");
    hooks::install();
    if rt.safe {
        return;
    }
    appbar::forget_registration();
    appbar::register(rt.hwnd());
    apply_taskbar_mode();
    request_reposition();
}

/// Tray "Reload": re-read settings, re-register with the shell, reload the UI.
pub fn reload() {
    let Some(rt) = runtime() else { return };
    let (_, settings) = rt.app.state::<SettingsStore>().reload();
    rt.state.lock().taskbar_suspended = false;
    if !rt.safe {
        appbar::unregister(rt.hwnd());
        appbar::register(rt.hwnd());
        apply_taskbar_mode();
    }
    hooks::install();
    reposition();
    let _ = rt.app.emit("settings://changed", settings);
    emit_app_info();
    for window in rt.app.webview_windows().values() {
        let _ = window.eval("window.location.reload()");
    }
    log::info!("dock: reloaded");
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect::new(0, 0, 2560, 1600);

    #[test]
    fn strip_depth_scales_with_dpi() {
        // Medium pill: 52 + 2 * 8 = 68 logical pixels.
        let pill = DockLayoutKind::Pill;
        assert_eq!(band_thickness_px(DockSize::Medium, pill, 1.0), 68);
        assert_eq!(band_thickness_px(DockSize::Medium, pill, 1.25), 85);
        assert_eq!(band_thickness_px(DockSize::Medium, pill, 1.5), 102);
        assert_eq!(band_thickness_px(DockSize::Small, pill, 1.0), 60);
        assert_eq!(band_thickness_px(DockSize::Large, pill, 1.5), 114);
    }

    #[test]
    fn full_width_bar_reserves_only_its_own_height() {
        let full = DockLayoutKind::Full;
        assert_eq!(band_thickness_px(DockSize::Medium, full, 1.0), 52);
        assert_eq!(band_thickness_px(DockSize::Medium, full, 1.5), 78);
        assert_eq!(gap_for(full), 0.0);
        assert_eq!(gap_for(DockLayoutKind::Pill), DOCK_GAP);
    }

    #[test]
    fn headroom_is_capped_by_the_monitor() {
        assert_eq!(headroom_px(1.5, 1600, 102), 960);
        // A short monitor can't give 640 logical pixels.
        assert_eq!(headroom_px(1.0, 600, 68), 532);
        assert_eq!(headroom_px(1.0, 50, 68), 0);
    }

    #[test]
    fn window_extends_from_the_strip_toward_the_screen_centre() {
        let bottom_band = Rect::new(0, 1498, 2560, 1600);
        assert_eq!(
            window_rect_for_band(Edge::Bottom, bottom_band, 960),
            Rect::new(0, 538, 2560, 1600)
        );
        let top_band = Rect::new(0, 0, 2560, 102);
        assert_eq!(
            window_rect_for_band(Edge::Top, top_band, 960),
            Rect::new(0, 0, 2560, 1062)
        );
    }

    #[test]
    fn strip_position_inside_the_window() {
        let band = Rect::new(0, 1498, 2560, 1600);
        let window = window_rect_for_band(Edge::Bottom, band, 960);
        assert_eq!(
            band_in_window(Edge::Bottom, window, band),
            Rect::new(0, 960, 2560, 1062)
        );

        let band = Rect::new(0, 0, 2560, 102);
        let window = window_rect_for_band(Edge::Top, band, 960);
        assert_eq!(
            band_in_window(Edge::Top, window, band),
            Rect::new(0, 0, 2560, 102)
        );
    }

    #[test]
    fn regions_split_into_the_bar_and_open_popovers() {
        let band = Rect::new(0, 960, 2560, 1074);
        let bar = Rect::new(400, 972, 2160, 1062);
        assert_eq!(split_regions(band, &[bar]), (Some(bar), false));
        // A popover above the strip.
        let popover = Rect::new(1700, 500, 2100, 950);
        assert_eq!(split_regions(band, &[bar, popover]), (Some(bar), true));
        // Nothing reported yet.
        assert_eq!(split_regions(band, &[]), (None, false));
    }

    #[test]
    fn edge_trigger_is_the_last_pixel_row_level_with_the_bar() {
        let bar = Rect::new(400, 1498, 2160, 1588);
        assert!(at_edge(Edge::Bottom, (1200, 1599), bar, MONITOR));
        assert!(!at_edge(Edge::Bottom, (1200, 1598), bar, MONITOR));
        assert!(!at_edge(Edge::Bottom, (100, 1599), bar, MONITOR));
        let bar = Rect::new(400, 12, 2160, 102);
        assert!(at_edge(Edge::Top, (1200, 0), bar, MONITOR));
        assert!(!at_edge(Edge::Top, (1200, 1), bar, MONITOR));
    }

    #[test]
    fn hovering_covers_the_bar_and_the_gap_to_the_edge() {
        let bar = Rect::new(400, 1498, 2160, 1588);
        let hover = hover_rect(Edge::Bottom, bar, MONITOR, 15);
        assert_eq!(hover, Rect::new(385, 1483, 2175, 1600));
        assert!(contains(hover, (1000, 1595)));
        assert!(!contains(hover, (1000, 1400)));
        let bar = Rect::new(400, 12, 2160, 102);
        assert_eq!(
            hover_rect(Edge::Top, bar, MONITOR, 15),
            Rect::new(385, 0, 2175, 117)
        );
    }

    #[test]
    fn sliding_away_moves_the_bar_just_past_the_edge() {
        let bar = Rect::new(400, 1498, 2160, 1588);
        let travel = travel_px(Edge::Bottom, bar, MONITOR);
        assert_eq!(travel, 103);
        assert!(bar.top + travel >= MONITOR.bottom);
        let bar = Rect::new(400, 12, 2160, 102);
        let travel = travel_px(Edge::Top, bar, MONITOR);
        assert!(bar.bottom - travel <= MONITOR.top);
    }

    #[test]
    fn safe_mode_strip_sits_inside_the_work_area() {
        // A 72px taskbar at the bottom: the dock sits directly above it.
        let work = Rect::new(0, 0, 2560, 1528);
        assert_eq!(
            band_in_work_area(Edge::Bottom, work, 102),
            Rect::new(0, 1426, 2560, 1528)
        );
        assert_eq!(
            band_in_work_area(Edge::Top, work, 102),
            Rect::new(0, 0, 2560, 102)
        );
    }

    #[test]
    fn drop_position_picks_the_nearer_edge() {
        assert_eq!(snap_edge(100, MONITOR), Edge::Top);
        assert_eq!(snap_edge(799, MONITOR), Edge::Top);
        assert_eq!(snap_edge(800, MONITOR), Edge::Bottom);
        assert_eq!(snap_edge(1590, MONITOR), Edge::Bottom);
        // A monitor above the primary has negative coordinates.
        let upper = Rect::new(0, -1080, 1920, 0);
        assert_eq!(snap_edge(-900, upper), Edge::Top);
        assert_eq!(snap_edge(-100, upper), Edge::Bottom);
    }
}
