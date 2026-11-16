//! A minimal append-only log for code that runs without Tauri's logger: the
//! watchdog, `--restore`, the panic hook and early startup. Best effort only;
//! logging must never be the reason a restore fails.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths;

pub fn line(component: &str, message: &str) {
    let dir = paths::log_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("shell.log"))
    {
        let _ = writeln!(
            file,
            "[unix {secs}] [pid {}] [{component}] {message}",
            std::process::id()
        );
    }
}
