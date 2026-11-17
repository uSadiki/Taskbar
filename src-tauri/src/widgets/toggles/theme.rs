//! Windows' light/dark mode. This is the one place besides autostart where
//! Glassdock writes to the registry: the two documented personalisation
//! values, followed by the broadcast that makes running apps pick them up.

use std::mem::size_of;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use windows::core::{PCWSTR, w};

const KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
/// Apps, and the system surfaces (taskbar, Start), each have their own value.
const VALUES: [PCWSTR; 2] = [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")];

fn read(value: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            KEY,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    Some(data)
}

/// True when apps are set to dark. A missing value means light, as in Windows.
pub fn is_dark() -> bool {
    read(VALUES[0]) == Some(0)
}

pub fn set_dark(dark: bool) -> bool {
    let light: u32 = if dark { 0 } else { 1 };
    let mut ok = true;
    for value in VALUES {
        let result = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                KEY,
                value,
                REG_DWORD.0,
                Some((&light as *const u32).cast()),
                size_of::<u32>() as u32,
            )
        };
        ok &= result.is_ok();
    }
    // Tell every window the colour set changed. A hung app must not hold
    // this up, so each gets a short timeout.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
    ok
}
