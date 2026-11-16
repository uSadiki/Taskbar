//! Reads the current desktop wallpaper settings through `IDesktopWallpaper`.
//!
//! Must be called on a thread with COM initialized.

use std::path::PathBuf;

use windows::Win32::System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::{
    DESKTOP_WALLPAPER_POSITION, DWPOS_CENTER, DWPOS_FILL, DWPOS_FIT, DWPOS_SPAN, DWPOS_STRETCH,
    DWPOS_TILE, DesktopWallpaper, IDesktopWallpaper,
};
use windows::core::{PCWSTR, PWSTR};

use super::slice::FitMode;
use crate::error::AppResult;
use crate::geometry::Rect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperInfo {
    /// The image shown on the monitor, or `None` for a solid colour.
    pub path: Option<PathBuf>,
    pub fit: FitMode,
    /// Desktop background colour, shown behind and around the image.
    pub background: [u8; 3],
    /// Bounding rectangle of all monitors.
    pub virtual_desktop: Rect,
}

fn fit_from(position: DESKTOP_WALLPAPER_POSITION) -> FitMode {
    match position {
        DWPOS_CENTER => FitMode::Center,
        DWPOS_TILE => FitMode::Tile,
        DWPOS_STRETCH => FitMode::Stretch,
        DWPOS_FIT => FitMode::Fit,
        DWPOS_SPAN => FitMode::Span,
        DWPOS_FILL => FitMode::Fill,
        _ => FitMode::Fill,
    }
}

/// Take ownership of a COM-allocated string.
unsafe fn take_string(value: PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    let text = unsafe { value.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(value.0 as *const _)) };
    text
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect::new(
        a.left.min(b.left),
        a.top.min(b.top),
        a.right.max(b.right),
        a.bottom.max(b.bottom),
    )
}

/// The wallpaper for the monitor whose rectangle is `monitor`.
pub fn query(monitor: Rect) -> AppResult<WallpaperInfo> {
    unsafe {
        let desktop: IDesktopWallpaper =
            CoCreateInstance(&DesktopWallpaper, None, CLSCTX_LOCAL_SERVER)?;

        // Find the wallpaper's id for our monitor by matching rectangles, and
        // collect the virtual desktop bounds on the way.
        let mut monitor_id: Option<String> = None;
        let mut virtual_desktop = monitor;
        for index in 0..desktop.GetMonitorDevicePathCount()? {
            let id = take_string(desktop.GetMonitorDevicePathAt(index)?);
            if id.is_empty() {
                continue;
            }
            let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            // Detached monitors are listed too and have no rectangle.
            let Ok(rect) = desktop.GetMonitorRECT(PCWSTR(wide.as_ptr())) else {
                continue;
            };
            let rect: Rect = rect.into();
            virtual_desktop = union(virtual_desktop, rect);
            if rect == monitor {
                monitor_id = Some(id);
            }
        }

        let path = match &monitor_id {
            Some(id) => {
                let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
                take_string(desktop.GetWallpaper(PCWSTR(wide.as_ptr()))?)
            }
            // No match (unusual): a null id returns the image shared by all
            // monitors, or an empty string if they differ.
            None => take_string(desktop.GetWallpaper(PCWSTR::null())?),
        };

        let color = desktop.GetBackgroundColor()?.0;
        Ok(WallpaperInfo {
            path: (!path.is_empty()).then(|| PathBuf::from(path)),
            fit: fit_from(desktop.GetPosition()?),
            // COLORREF is 0x00BBGGRR.
            background: [
                (color & 0xFF) as u8,
                ((color >> 8) & 0xFF) as u8,
                ((color >> 16) & 0xFF) as u8,
            ],
            virtual_desktop,
        })
    }
}

/// The copy of the current wallpaper that Windows keeps for itself. Used when
/// the original file is unreadable (deleted, on an unplugged drive).
pub fn transcoded_copy() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("APPDATA")?)
        .join(r"Microsoft\Windows\Themes\TranscodedWallpaper");
    path.is_file().then_some(path)
}

/// The folder Windows rewrites whenever the wallpaper changes.
pub fn themes_dir() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("APPDATA")?).join(r"Microsoft\Windows\Themes");
    path.is_dir().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_position() {
        assert_eq!(fit_from(DWPOS_CENTER), FitMode::Center);
        assert_eq!(fit_from(DWPOS_TILE), FitMode::Tile);
        assert_eq!(fit_from(DWPOS_STRETCH), FitMode::Stretch);
        assert_eq!(fit_from(DWPOS_FIT), FitMode::Fit);
        assert_eq!(fit_from(DWPOS_FILL), FitMode::Fill);
        assert_eq!(fit_from(DWPOS_SPAN), FitMode::Span);
    }

    #[test]
    fn union_bounds_both_rectangles() {
        let a = Rect::new(0, 0, 1920, 1080);
        let b = Rect::new(-2560, -200, 0, 1240);
        assert_eq!(union(a, b), Rect::new(-2560, -200, 1920, 1240));
    }
}
