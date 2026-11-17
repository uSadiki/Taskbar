//! Shared plumbing for the widgets that talk to the internet: one HTTP
//! client, and a small on-disk cache so a widget has something to show at
//! startup and while offline.
//!
//! All requests are made here in Rust. Tokens never reach the webview.

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::paths;

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("Glassdock/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(8))
            .build()
            // Building only fails if the TLS backend can't start; a default
            // client then reports the same failure per request.
            .unwrap_or_default()
    })
}

/// A short, user-facing reason for a failed request.
pub fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "The request timed out".into()
    } else if error.is_connect() {
        "No connection".into()
    } else if let Some(status) = error.status() {
        format!("The server answered {}", status.as_u16())
    } else {
        "The request failed".into()
    }
}

fn cache_path(name: &str) -> PathBuf {
    paths::local_dir()
        .join("cache")
        .join(format!("{name}.json"))
}

pub fn cache_read<T: DeserializeOwned>(name: &str) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(cache_path(name)).ok()?).ok()
}

pub fn cache_write<T: Serialize>(name: &str, value: &T) {
    let path = cache_path(name);
    let Ok(json) = serde_json::to_vec(value) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    // Write beside the target and rename, so a crash can't leave half a file.
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

pub fn cache_remove(name: &str) {
    let _ = fs::remove_file(cache_path(name));
}

/// Seconds since the Unix epoch.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Open an https link in the default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, w};

    let parsed = reqwest::Url::parse(url).map_err(|_| "Not a valid link".to_string())?;
    if parsed.scheme() != "https" {
        return Err("Only https links can be opened".into());
    }
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(parsed.as_str()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute reports success with a value above 32.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("The link could not be opened".into())
    }
}
