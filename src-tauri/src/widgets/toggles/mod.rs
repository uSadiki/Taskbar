//! Quick Toggles: radios, audio, brightness and light/dark mode.

pub mod audio;
pub mod brightness;
pub mod radios;
pub mod theme;

use serde::Serialize;
use tauri::AppHandle;

use crate::shell::worker;

/// Everything the Quick Toggles popover shows. Radios and audio also push
/// their own change events; brightness and dark mode are read when asked.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TogglesSnapshot {
    pub radios: radios::RadiosState,
    pub audio: audio::AudioState,
    /// `None` when no display supports brightness control.
    pub brightness: Option<u8>,
    pub dark_mode: bool,
}

pub fn start(app: &AppHandle) {
    radios::start(app);
    audio::start(app);
}

/// Gather the current state. Blocks on the shell worker; call it from an
/// async command, not from the main thread.
pub fn snapshot() -> TogglesSnapshot {
    let brightness = worker::call(brightness::get).flatten();
    TogglesSnapshot {
        radios: radios::state(),
        audio: audio::state(),
        brightness,
        dark_mode: theme::is_dark(),
    }
}

pub fn set_brightness(percent: u8) {
    worker::post(move || {
        if !brightness::set(percent) {
            log::warn!("brightness: could not be set");
        }
    });
}
