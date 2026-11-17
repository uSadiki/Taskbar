//! Start at login: a `Glassdock` value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, written by the
//! autostart plugin. One of the registry writes the brief allows.

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteKeyValueW};
use windows::core::w;

/// Make the Run entry match the setting. Development builds never register
/// themselves, so login never starts a build-folder executable.
pub fn sync(app: &AppHandle, enabled: bool) {
    if cfg!(debug_assertions) {
        if enabled {
            log::info!("autostart: development build, not registering");
        }
        return;
    }
    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(false);
    let result = match (enabled, current) {
        (true, false) => manager.enable(),
        (false, true) => manager.disable(),
        // Re-register even when enabled, in case the app moved.
        (true, true) => manager.enable(),
        (false, false) => Ok(()),
    };
    match result {
        Ok(()) => log::info!("autostart: {}", if enabled { "on" } else { "off" }),
        Err(e) => log::warn!("autostart: could not be changed: {e}"),
    }
}

/// Remove the Run entry without the plugin, for the uninstaller.
pub fn remove() {
    unsafe {
        let _ = RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            w!("Glassdock"),
        );
        let _ = RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"),
            w!("Glassdock"),
        );
    }
}
