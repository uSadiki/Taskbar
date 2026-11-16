//! Small Win32 helpers shared by the shell modules.

use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowRect};

use crate::geometry::Rect;

/// Window handles are stored as integers so shared state stays `Send + Sync`.
pub fn hwnd_to_isize(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

pub fn hwnd_from_isize(value: isize) -> HWND {
    HWND(value as *mut c_void)
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    Some(rect.into())
}
