//! Wi-Fi and Bluetooth on/off, through `Windows.Devices.Radios`.
//!
//! This API only finds radios when the process has the same architecture as
//! Windows, which holds for the 64-bit build. If Windows denies access, the
//! toggles report `None` and the UI offers the settings page instead.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Foundation::TypedEventHandler;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiosState {
    /// `None`: this PC has no such radio, or Windows denied control of it.
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RadioName {
    Wifi,
    Bluetooth,
}

impl RadioName {
    fn kind(self) -> RadioKind {
        match self {
            RadioName::Wifi => RadioKind::WiFi,
            RadioName::Bluetooth => RadioKind::Bluetooth,
        }
    }
}

/// Radio objects are agile (free-threaded) WinRT objects.
struct Radios(Vec<Radio>);
// SAFETY: `Radio` is marked agile in its WinRT metadata, so it may be used
// from any thread.
unsafe impl Send for Radios {}

static RADIOS: Mutex<Option<Radios>> = Mutex::new(None);

fn is_on(radio: &Radio) -> bool {
    radio.State().is_ok_and(|s| s == RadioState::On)
}

pub fn state() -> RadiosState {
    let guard = RADIOS.lock();
    let Some(radios) = guard.as_ref() else {
        return RadiosState::default();
    };
    let find = |kind: RadioKind| {
        radios
            .0
            .iter()
            .find(|r| r.Kind().is_ok_and(|k| k == kind))
            .map(is_on)
    };
    RadiosState {
        wifi: find(RadioKind::WiFi),
        bluetooth: find(RadioKind::Bluetooth),
    }
}

fn load(app: &AppHandle) -> windows::core::Result<()> {
    let access = Radio::RequestAccessAsync()?.join()?;
    if access != RadioAccessStatus::Allowed {
        log::warn!("radios: Windows did not allow radio control ({access:?})");
        return Ok(());
    }
    let radios: Vec<Radio> = Radio::GetRadiosAsync()?.join()?.into_iter().collect();
    for radio in &radios {
        let app = app.clone();
        // Fires for changes made anywhere: here, in Settings, by a hardware key.
        let _ = radio.StateChanged(&TypedEventHandler::new(move |_, _| {
            let _ = app.emit("radios://changed", state());
            Ok(())
        }));
    }
    log::info!("radios: {} radio(s) found", radios.len());
    *RADIOS.lock() = Some(Radios(radios));
    let _ = app.emit("radios://changed", state());
    Ok(())
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("radios".into())
        .spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            if let Err(e) = load(&app) {
                log::warn!("radios: unavailable: {e}");
            }
        });
}

/// Turn a radio on or off. The result arrives as a `radios://changed` event.
pub fn set(name: RadioName, on: bool) {
    let radio = RADIOS.lock().as_ref().and_then(|radios| {
        radios
            .0
            .iter()
            .find(|r| r.Kind().is_ok_and(|k| k == name.kind()))
            .cloned()
    });
    let Some(radio) = radio.map(|r| Radios(vec![r])) else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("radio-set".into())
        .spawn(move || {
            let target = if on { RadioState::On } else { RadioState::Off };
            let result = radio.0[0].SetStateAsync(target).and_then(|op| op.join());
            match result {
                Ok(RadioAccessStatus::Allowed) => {}
                Ok(other) => log::warn!("radios: {name:?} could not be switched ({other:?})"),
                Err(e) => log::warn!("radios: {name:?} could not be switched: {e}"),
            }
        });
}
