//! Where Glassdock keeps its files.
//!
//! These are resolved from environment variables rather than through Tauri so
//! the watchdog and `--restore` modes, which never start Tauri, see the same
//! paths as the app.

use std::path::PathBuf;

const APP_DIR: &str = "Glassdock";

fn base(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

/// `%LOCALAPPDATA%\Glassdock`: logs, caches and the taskbar restore journal.
pub fn local_dir() -> PathBuf {
    base("LOCALAPPDATA")
}

/// `%APPDATA%\Glassdock`: settings.
pub fn roaming_dir() -> PathBuf {
    base("APPDATA")
}

pub fn log_dir() -> PathBuf {
    local_dir().join("logs")
}

pub fn journal_path() -> PathBuf {
    local_dir().join("taskbar-restore.json")
}

pub fn settings_path() -> PathBuf {
    roaming_dir().join("settings.json")
}

pub fn webview_dir() -> PathBuf {
    local_dir().join("webview")
}
