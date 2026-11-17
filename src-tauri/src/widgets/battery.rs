//! Battery level, charging state and time estimates.
//!
//! Updated from power notifications Windows sends to the dock window; there
//! is no timer.

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Devices::Power::Battery;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::Power::{
    GetSystemPowerStatus, RegisterPowerSettingNotification, SYSTEM_POWER_STATUS,
};
use windows::Win32::System::SystemServices::{
    GUID_ACDC_POWER_SOURCE, GUID_BATTERY_PERCENTAGE_REMAINING, GUID_POWER_SAVING_STATUS,
};
use windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_WINDOW_HANDLE;

use crate::shell::worker;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryState {
    /// False on a PC without a battery; the widget hides itself.
    pub present: bool,
    pub percent: Option<u8>,
    /// On mains power.
    pub plugged: bool,
    pub charging: bool,
    /// Windows' battery saver is on.
    pub saver: bool,
    pub seconds_remaining: Option<u32>,
    pub seconds_to_full: Option<u32>,
}

/// What `GetSystemPowerStatus` reports.
#[derive(Debug, Clone, Copy)]
pub struct RawPower {
    pub ac_line: u8,
    pub flag: u8,
    pub percent: u8,
    pub saver: u8,
    pub life_seconds: u32,
}

/// What the aggregate battery report adds, in milliwatts and milliwatt-hours.
#[derive(Debug, Clone, Copy, Default)]
pub struct RawReport {
    pub charge_rate: Option<i32>,
    pub remaining: Option<i32>,
    pub full: Option<i32>,
}

const FLAG_CHARGING: u8 = 8;
const FLAG_NO_BATTERY: u8 = 128;
const UNKNOWN: u8 = 255;

/// Turn the raw readings into what the widget shows.
pub fn derive(power: RawPower, report: RawReport) -> BatteryState {
    let present = power.flag != UNKNOWN && power.flag & FLAG_NO_BATTERY == 0;
    if !present {
        return BatteryState {
            plugged: power.ac_line == 1,
            ..Default::default()
        };
    }
    let plugged = power.ac_line == 1;
    let charging = power.flag & FLAG_CHARGING != 0;

    // Time to full, from how fast the battery is filling.
    let seconds_to_full = match (charging, report.charge_rate, report.remaining, report.full) {
        (true, Some(rate), Some(remaining), Some(full)) if rate > 0 && full > remaining => {
            Some(((i64::from(full - remaining) * 3600) / i64::from(rate)) as u32)
        }
        _ => None,
    };
    // Windows reports u32::MAX while it is still estimating, or on mains.
    let seconds_remaining =
        (!plugged && power.life_seconds != u32::MAX).then_some(power.life_seconds);

    BatteryState {
        present,
        percent: (power.percent != UNKNOWN).then_some(power.percent.min(100)),
        plugged,
        charging,
        saver: power.saver == 1,
        seconds_remaining,
        seconds_to_full,
    }
}

fn read() -> BatteryState {
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return BatteryState::default();
    }
    let power = RawPower {
        ac_line: status.ACLineStatus,
        flag: status.BatteryFlag,
        percent: status.BatteryLifePercent,
        saver: status.SystemStatusFlag,
        life_seconds: status.BatteryLifeTime,
    };
    let report = Battery::AggregateBattery()
        .and_then(|battery| battery.GetReport())
        .map(|report| RawReport {
            charge_rate: report.ChargeRateInMilliwatts().and_then(|v| v.Value()).ok(),
            remaining: report
                .RemainingCapacityInMilliwattHours()
                .and_then(|v| v.Value())
                .ok(),
            full: report
                .FullChargeCapacityInMilliwattHours()
                .and_then(|v| v.Value())
                .ok(),
        })
        .unwrap_or_default();
    derive(power, report)
}

static STATE: Mutex<Option<BatteryState>> = Mutex::new(None);

pub fn state() -> BatteryState {
    STATE.lock().clone().unwrap_or_default()
}

/// Re-read the battery and tell the UI if anything changed.
pub fn refresh(app: &AppHandle) {
    let app = app.clone();
    worker::post(move || {
        let next = read();
        let changed = STATE.lock().replace(next.clone()).as_ref() != Some(&next);
        if changed {
            let _ = app.emit("battery://changed", next);
        }
    });
}

/// Ask Windows to notify the dock window about battery level, power source
/// and battery saver changes, then take the first reading.
pub fn start(app: &AppHandle, hwnd: HWND) {
    for setting in [
        &GUID_BATTERY_PERCENTAGE_REMAINING,
        &GUID_ACDC_POWER_SOURCE,
        &GUID_POWER_SAVING_STATUS,
    ] {
        // The registration lives as long as the window; nothing to keep.
        let _ = unsafe {
            RegisterPowerSettingNotification(HANDLE(hwnd.0), setting, DEVICE_NOTIFY_WINDOW_HANDLE)
        };
    }
    refresh(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn power(ac_line: u8, flag: u8, percent: u8, life_seconds: u32) -> RawPower {
        RawPower {
            ac_line,
            flag,
            percent,
            saver: 0,
            life_seconds,
        }
    }

    #[test]
    fn desktop_without_a_battery() {
        let state = derive(
            power(1, FLAG_NO_BATTERY, UNKNOWN, u32::MAX),
            RawReport::default(),
        );
        assert!(!state.present && state.plugged);
        assert_eq!(state.percent, None);
    }

    #[test]
    fn unknown_battery_state_counts_as_no_battery() {
        assert!(
            !derive(
                power(UNKNOWN, UNKNOWN, UNKNOWN, u32::MAX),
                RawReport::default()
            )
            .present
        );
    }

    #[test]
    fn on_battery_with_an_estimate() {
        let state = derive(power(0, 1, 76, 3 * 3600 + 600), RawReport::default());
        assert!(state.present && !state.plugged && !state.charging);
        assert_eq!(state.percent, Some(76));
        assert_eq!(state.seconds_remaining, Some(11_400));
        assert_eq!(state.seconds_to_full, None);
    }

    #[test]
    fn on_battery_while_windows_is_still_estimating() {
        assert_eq!(
            derive(power(0, 1, 76, u32::MAX), RawReport::default()).seconds_remaining,
            None
        );
    }

    #[test]
    fn charging_time_comes_from_the_charge_rate() {
        // 30 Wh to go at 45 W: 40 minutes.
        let report = RawReport {
            charge_rate: Some(45_000),
            remaining: Some(40_000),
            full: Some(70_000),
        };
        let state = derive(power(1, FLAG_CHARGING, 57, u32::MAX), report);
        assert!(state.charging && state.plugged);
        assert_eq!(state.seconds_to_full, Some(2400));
        assert_eq!(state.seconds_remaining, None);
    }

    #[test]
    fn plugged_in_and_full_has_no_estimates() {
        let report = RawReport {
            charge_rate: Some(0),
            remaining: Some(70_000),
            full: Some(70_000),
        };
        let state = derive(power(1, 1, 100, u32::MAX), report);
        assert!(!state.charging);
        assert_eq!(
            (state.seconds_to_full, state.seconds_remaining),
            (None, None)
        );
    }

    #[test]
    fn a_negative_charge_rate_gives_no_time_to_full() {
        let report = RawReport {
            charge_rate: Some(-5_000),
            remaining: Some(10),
            full: Some(100),
        };
        assert_eq!(
            derive(power(1, FLAG_CHARGING, 10, u32::MAX), report).seconds_to_full,
            None
        );
    }

    #[test]
    fn battery_saver_and_out_of_range_percent() {
        let mut raw = power(0, 1, 101, 600);
        raw.saver = 1;
        let state = derive(raw, RawReport::default());
        assert!(state.saver);
        assert_eq!(state.percent, Some(100));
    }
}
