//! The backdrop behind the glass: the desktop wallpaper exactly as Windows
//! paints it on the dock's monitor, as raw pixels for the WebGL renderer.
//!
//! The dock is an appbar, so the only thing ever behind it is the wallpaper.
//! Rendering the wallpaper ourselves is what lets the glass refract it.

pub mod decode;
pub mod slice;
pub mod wallpaper;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

use self::decode::Image;
use self::slice::FitMode;
use crate::error::AppResult;
use crate::geometry::Rect;
use crate::shell::dock;

/// Everything the backdrop depends on. If none of it changed, the pixels
/// didn't either and the refresh is skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Signature {
    path: Option<PathBuf>,
    modified: Option<SystemTime>,
    len: u64,
    fit: FitMode,
    background: [u8; 3],
    monitor: Rect,
    virtual_desktop: Rect,
}

pub struct Backdrop {
    pub rev: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Sent with `glass://backdrop` so the frontend knows to fetch new pixels.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackdropInfo {
    pub rev: u32,
    pub width: u32,
    pub height: u32,
    /// False when the wallpaper could not be read; the frontend then falls
    /// back to a plain translucent fill.
    pub available: bool,
    pub error: Option<String>,
}

struct State {
    backdrop: Option<Arc<Backdrop>>,
    signature: Option<Signature>,
    rev: u32,
    error: Option<String>,
    /// A refresh thread is running.
    refreshing: bool,
    /// Another refresh was requested while one was running.
    again: bool,
    watcher: Option<RecommendedWatcher>,
}

static STATE: Mutex<State> = Mutex::new(State {
    backdrop: None,
    signature: None,
    rev: 0,
    error: None,
    refreshing: false,
    again: false,
    watcher: None,
});

pub fn info() -> BackdropInfo {
    let state = STATE.lock();
    match &state.backdrop {
        Some(b) => BackdropInfo {
            rev: b.rev,
            width: b.width,
            height: b.height,
            available: true,
            error: None,
        },
        None => BackdropInfo {
            rev: state.rev,
            width: 0,
            height: 0,
            available: false,
            error: state.error.clone(),
        },
    }
}

/// The current backdrop as one buffer: a 16-byte header (revision, width,
/// height, reserved; little-endian u32 each) followed by RGBA pixels. Width
/// and height are zero when there is no backdrop.
pub fn packed() -> Vec<u8> {
    let backdrop = STATE.lock().backdrop.clone();
    let (rev, width, height, pixels): (u32, u32, u32, &[u8]) = match &backdrop {
        Some(b) => (b.rev, b.width, b.height, &b.rgba),
        None => (0, 0, 0, &[]),
    };
    let mut out = Vec::with_capacity(16 + pixels.len());
    for value in [rev, width, height, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(pixels);
    out
}

/// Re-read the wallpaper soon. Safe to call often and from any thread: calls
/// during a refresh collapse into one follow-up refresh.
pub fn request_refresh(app: &AppHandle) {
    {
        let mut state = STATE.lock();
        if state.refreshing {
            state.again = true;
            return;
        }
        state.refreshing = true;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            // Wallpaper changes arrive as a burst of notifications; let them
            // settle so the file is fully written before it is read.
            std::thread::sleep(Duration::from_millis(300));
            STATE.lock().again = false;
            if refresh_once() {
                let _ = app.emit("glass://backdrop", info());
            }
            let mut state = STATE.lock();
            if !state.again {
                state.refreshing = false;
                break;
            }
        }
    });
}

/// Returns true if the backdrop (or its availability) changed.
fn refresh_once() -> bool {
    let Some(monitor) = dock::monitor_rect() else {
        return false;
    };

    // A short-lived apartment: created for this refresh and torn down after,
    // so no idle thread is left owning COM windows without a message loop.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let result = render(monitor);
    if initialized {
        unsafe { CoUninitialize() };
    }

    let mut state = STATE.lock();
    match result {
        Ok(None) => false,
        Ok(Some((signature, image))) => {
            state.rev = state.rev.wrapping_add(1);
            log::info!(
                "glass: backdrop {} is {}x{} from {} ({:?})",
                state.rev,
                image.width,
                image.height,
                signature
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "a solid colour".into()),
                signature.fit
            );
            state.backdrop = Some(Arc::new(Backdrop {
                rev: state.rev,
                width: image.width,
                height: image.height,
                rgba: image.rgba,
            }));
            state.signature = Some(signature);
            state.error = None;
            true
        }
        Err(e) => {
            let message = e.to_string();
            let changed = state.backdrop.is_some() || state.error.as_deref() != Some(&message);
            if changed {
                log::warn!(
                    "glass: the wallpaper could not be read ({message}); using the fallback"
                );
                state.rev = state.rev.wrapping_add(1);
            }
            state.backdrop = None;
            state.signature = None;
            state.error = Some(message);
            changed
        }
    }
}

fn render(monitor: Rect) -> AppResult<Option<(Signature, Image)>> {
    let wallpaper = wallpaper::query(monitor)?;
    let metadata = wallpaper
        .path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok());
    let signature = Signature {
        path: wallpaper.path.clone(),
        modified: metadata.as_ref().and_then(|m| m.modified().ok()),
        len: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
        fit: wallpaper.fit,
        background: wallpaper.background,
        monitor,
        virtual_desktop: wallpaper.virtual_desktop,
    };
    if STATE.lock().signature.as_ref() == Some(&signature) {
        return Ok(None);
    }

    let paint = |path: &std::path::Path| {
        decode::render(
            path,
            wallpaper.fit,
            wallpaper.background,
            monitor,
            wallpaper.virtual_desktop,
        )
    };
    let image = match &wallpaper.path {
        None => Image::solid(
            monitor.width().max(1) as u32,
            monitor.height().max(1) as u32,
            wallpaper.background,
        ),
        Some(path) => match paint(path) {
            Ok(image) => image,
            Err(original) => match wallpaper::transcoded_copy() {
                Some(copy) => {
                    log::warn!(
                        "glass: {} could not be decoded ({original}); using Windows' cached copy",
                        path.display()
                    );
                    paint(&copy)?
                }
                None => return Err(original),
            },
        },
    };
    Ok(Some((signature, image)))
}

/// Watch the folder Windows rewrites on every wallpaper change. This backs up
/// the `WM_SETTINGCHANGE` notification, which some slideshow changes skip.
pub fn start_watching(app: &AppHandle) {
    let Some(dir) = wallpaper::themes_dir() else {
        return;
    };
    let handle = app.clone();
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            request_refresh(&handle);
        }
    })
    .and_then(|mut watcher| {
        watcher.watch(&dir, RecursiveMode::Recursive)?;
        Ok(watcher)
    });
    match watcher {
        Ok(watcher) => STATE.lock().watcher = Some(watcher),
        Err(e) => log::warn!(
            "glass: cannot watch {} for wallpaper changes: {e}",
            dir.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_header_describes_an_empty_backdrop() {
        // No backdrop has been rendered in a unit test process.
        let bytes = packed();
        assert_eq!(bytes.len(), 16);
        assert!(bytes.iter().all(|b| *b == 0));
        assert!(!info().available);
    }
}
