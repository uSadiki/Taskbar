//! Monitor enumeration and the choice of which monitor hosts the dock.

use std::mem::size_of;

use serde::Serialize;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::BOOL;

use crate::geometry::Rect;

const MONITORINFOF_PRIMARY: u32 = 0x1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    /// Device name, for example `\\.\DISPLAY1`. Stable across reboots for a
    /// given port, which is what the monitor setting stores.
    pub id: String,
    /// Full monitor rectangle in physical pixels.
    pub rect: Rect,
    /// Monitor rectangle minus appbars (including this dock, once registered).
    pub work: Rect,
    /// Effective DPI; 96 is 100% scaling.
    pub dpi: u32,
    pub primary: bool,
}

unsafe extern "system" fn collect(
    monitor: HMONITOR,
    _hdc: HDC,
    _clip: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the address of the Vec passed to EnumDisplayMonitors
    // below, which outlives the enumeration.
    let list = unsafe { &mut *(data.0 as *mut Vec<MonitorInfo>) };

    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe {
        GetMonitorInfoW(
            monitor,
            &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
        )
    };
    if ok.as_bool() {
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_err()
        {
            dpi_x = 96;
        }
        let name_len = info
            .szDevice
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(info.szDevice.len());
        list.push(MonitorInfo {
            id: String::from_utf16_lossy(&info.szDevice[..name_len]),
            rect: info.monitorInfo.rcMonitor.into(),
            work: info.monitorInfo.rcWork.into(),
            dpi: dpi_x.max(1),
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    BOOL(1)
}

pub fn enumerate() -> Vec<MonitorInfo> {
    let mut list: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut list as *mut Vec<MonitorInfo> as isize),
        );
    }
    list
}

/// The monitor the dock should use: the configured one if it is connected,
/// otherwise the primary, otherwise whatever exists.
pub fn pick<'a>(monitors: &'a [MonitorInfo], wanted: Option<&str>) -> Option<&'a MonitorInfo> {
    wanted
        .and_then(|id| monitors.iter().find(|m| m.id.eq_ignore_ascii_case(id)))
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or_else(|| monitors.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: &str, primary: bool) -> MonitorInfo {
        MonitorInfo {
            id: id.into(),
            rect: Rect::new(0, 0, 1920, 1080),
            work: Rect::new(0, 0, 1920, 1032),
            dpi: 96,
            primary,
        }
    }

    #[test]
    fn prefers_the_configured_monitor() {
        let list = [
            monitor(r"\\.\DISPLAY1", true),
            monitor(r"\\.\DISPLAY2", false),
        ];
        assert_eq!(
            pick(&list, Some(r"\\.\display2")).unwrap().id,
            r"\\.\DISPLAY2"
        );
    }

    #[test]
    fn falls_back_to_primary_when_the_configured_monitor_is_unplugged() {
        let list = [
            monitor(r"\\.\DISPLAY3", false),
            monitor(r"\\.\DISPLAY1", true),
        ];
        assert_eq!(
            pick(&list, Some(r"\\.\DISPLAY2")).unwrap().id,
            r"\\.\DISPLAY1"
        );
        assert_eq!(pick(&list, None).unwrap().id, r"\\.\DISPLAY1");
    }

    #[test]
    fn falls_back_to_any_monitor_and_handles_none() {
        let list = [monitor(r"\\.\DISPLAY5", false)];
        assert_eq!(pick(&list, None).unwrap().id, r"\\.\DISPLAY5");
        assert!(pick(&[], Some("x")).is_none());
    }
}
