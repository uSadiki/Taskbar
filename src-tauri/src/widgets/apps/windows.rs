//! The open windows that belong in the dock: the same ones Alt-Tab shows.
//!
//! Must be called on the shell worker thread (window properties are read
//! through COM).

use std::mem::size_of;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromParsingName, SIGDN_NORMALDISPLAY};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR, w};

use crate::shell::win::{class_name, hwnd_to_isize};

/// Top-level windows that are shell furniture rather than apps.
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "MultitaskingViewFrame",
    "ForegroundStaging",
];

/// What identifies a window's app. Read once per window and cached.
#[derive(Debug, Clone)]
pub struct Identity {
    pub exe: String,
    pub aumid: Option<String>,
    pub app_name: String,
}

pub fn title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

pub fn is_minimized(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) }.as_bool()
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

/// The Alt-Tab rule: visible, not a tool window, not owned (unless it asks
/// for a taskbar button), not cloaked (hidden Store apps, other virtual
/// desktops), and titled.
fn belongs_in_dock(hwnd: HWND, own_pid: u32) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let wants_button = ex & WS_EX_APPWINDOW.0 != 0;
        if !wants_button && ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0 {
            return false;
        }
        if !wants_button && GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid()) {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == own_pid || is_cloaked(hwnd) {
            return false;
        }
    }
    !title(hwnd).is_empty() && !SHELL_CLASSES.contains(&class_name(hwnd).as_str())
}

unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the address of the Vec passed to EnumWindows below.
    let list = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
    list.push(hwnd);
    BOOL(1)
}

/// Handles of the windows that get an icon or a dot in the dock.
pub fn enumerate() -> Vec<isize> {
    let mut all: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize));
    }
    let own_pid = std::process::id();
    all.into_iter()
        .filter(|hwnd| belongs_in_dock(*hwnd, own_pid))
        .map(hwnd_to_isize)
        .collect()
}

unsafe fn take(value: PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    let text = unsafe { value.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(value.0 as *const _)) };
    text
}

/// The AppUserModelID the window itself declares, if any.
fn window_aumid(hwnd: HWND) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let mut value = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let text = PropVariantToStringAlloc(&value).ok().map(|p| take(p));
        let _ = PropVariantClear(&mut value);
        text.filter(|t| !t.is_empty())
    }
}

/// Executable path and, for packaged apps, the process's AppUserModelID.
fn process_identity(hwnd: HWND) -> (String, Option<String>) {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return (String::new(), None);
        };

        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        let exe = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
        .ok()
        .map(|()| String::from_utf16_lossy(&buffer[..len as usize]))
        .unwrap_or_default();

        let mut id = [0u16; 256];
        let mut id_len = id.len() as u32;
        let aumid =
            (GetApplicationUserModelId(process, &mut id_len, Some(PWSTR(id.as_mut_ptr()))).0 == 0)
                .then(|| {
                    let end = id.iter().position(|c| *c == 0).unwrap_or(0);
                    String::from_utf16_lossy(&id[..end])
                })
                .filter(|t| !t.is_empty());

        let _ = CloseHandle(process);
        (exe, aumid)
    }
}

/// The app's display name from its executable's version resource.
fn file_description(exe: &str) -> Option<String> {
    unsafe {
        let path = HSTRING::from(exe);
        let size = GetFileVersionInfoSizeW(&path, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&path, None, size, data.as_mut_ptr().cast()).ok()?;

        // The first language/code-page pair the file declares.
        let mut pointer = std::ptr::null_mut();
        let mut len = 0u32;
        let found = VerQueryValueW(
            data.as_ptr().cast(),
            w!(r"\VarFileInfo\Translation"),
            &mut pointer,
            &mut len,
        );
        if !found.as_bool() || len < 4 || pointer.is_null() {
            return None;
        }
        let pair = std::slice::from_raw_parts(pointer as *const u16, 2);
        let query: Vec<u16> = format!(
            r"\StringFileInfo\{:04x}{:04x}\FileDescription",
            pair[0], pair[1]
        )
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

        let mut text = std::ptr::null_mut();
        let mut text_len = 0u32;
        let found = VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(query.as_ptr()),
            &mut text,
            &mut text_len,
        );
        if !found.as_bool() || text.is_null() || text_len == 0 {
            return None;
        }
        let chars = std::slice::from_raw_parts(text as *const u16, text_len as usize);
        let end = chars.iter().position(|c| *c == 0).unwrap_or(chars.len());
        Some(String::from_utf16_lossy(&chars[..end]).trim().to_string()).filter(|t| !t.is_empty())
    }
}

/// A packaged app's display name, from its entry in the Apps folder.
fn packaged_name(aumid: &str) -> Option<String> {
    unsafe {
        let path = HSTRING::from(format!(r"shell:AppsFolder\{aumid}"));
        let item: IShellItem = SHCreateItemFromParsingName(&path, None).ok()?;
        Some(take(item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?)).filter(|t| !t.is_empty())
    }
}

fn file_stem(exe: &str) -> String {
    let name = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    name.strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name)
        .to_string()
}

pub fn identity(hwnd: HWND) -> Identity {
    let (exe, process_aumid) = process_identity(hwnd);
    let aumid = window_aumid(hwnd).or(process_aumid);
    let app_name = aumid
        .as_deref()
        .and_then(packaged_name)
        .or_else(|| file_description(&exe))
        .unwrap_or_else(|| file_stem(&exe));
    Identity {
        exe,
        aumid,
        app_name,
    }
}

/// `MAX_PATH` is referenced so long paths are a deliberate choice above: the
/// buffers are larger than it on purpose.
const _: () = assert!(MAX_PATH < 1024);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_stem_drops_the_directory_and_extension() {
        assert_eq!(file_stem(r"C:\Windows\notepad.exe"), "notepad");
        assert_eq!(file_stem(r"C:\Tools\APP.EXE"), "APP");
        assert_eq!(file_stem("weird"), "weird");
    }
}
