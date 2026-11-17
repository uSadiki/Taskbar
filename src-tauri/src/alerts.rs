//! The dock's own alerts, shown as toasts above the dock.
//!
//! Alerts are held, not shown, while Quiet is on (Quick Toggles) or during
//! the quiet-hours schedule. A held alert is not lost: the source raises it
//! again on its next check, and it shows once quiet time ends.

use chrono::{Local, NaiveTime};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{Notifications, SettingsStore};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    /// Stable id; showing the same id again replaces the toast.
    pub id: String,
    pub title: String,
    pub body: String,
    /// "info", "warning" or "critical".
    pub level: String,
    /// The popover to open when the toast is clicked.
    pub open: Option<String>,
}

fn parse_time(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M").ok()
}

/// Is `now` inside the quiet-hours schedule? A schedule whose end is earlier
/// than its start runs overnight. Equal start and end means all day.
pub fn in_quiet_hours(now: NaiveTime, start: &str, end: &str) -> bool {
    let (Some(start), Some(end)) = (parse_time(start), parse_time(end)) else {
        return false;
    };
    if start == end {
        true
    } else if start < end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

pub fn is_quiet(settings: &Notifications, now: NaiveTime) -> bool {
    settings.muted
        || (settings.quiet_hours.enabled
            && in_quiet_hours(now, &settings.quiet_hours.start, &settings.quiet_hours.end))
}

/// Show an alert unless it's quiet time. Returns whether it was shown, so the
/// caller can try again later if it wasn't.
pub fn show(app: &AppHandle, alert: Alert) -> bool {
    let settings = app.state::<SettingsStore>().get().notifications;
    if is_quiet(&settings, Local::now().time()) {
        log::info!("alerts: held during quiet time: {}", alert.title);
        return false;
    }
    log::info!("alerts: {}", alert.title);
    app.emit("alert://show", alert).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::QuietHours;

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn daytime_schedule() {
        assert!(in_quiet_hours(at(13, 0), "12:00", "14:00"));
        assert!(in_quiet_hours(at(12, 0), "12:00", "14:00"));
        assert!(!in_quiet_hours(at(14, 0), "12:00", "14:00"));
        assert!(!in_quiet_hours(at(9, 0), "12:00", "14:00"));
    }

    #[test]
    fn overnight_schedule() {
        assert!(in_quiet_hours(at(23, 30), "22:00", "07:00"));
        assert!(in_quiet_hours(at(3, 0), "22:00", "07:00"));
        assert!(!in_quiet_hours(at(7, 0), "22:00", "07:00"));
        assert!(!in_quiet_hours(at(15, 0), "22:00", "07:00"));
    }

    #[test]
    fn equal_times_mean_all_day_and_bad_times_mean_never() {
        assert!(in_quiet_hours(at(15, 0), "08:00", "08:00"));
        assert!(!in_quiet_hours(at(15, 0), "nope", "08:00"));
    }

    #[test]
    fn quiet_switch_or_schedule() {
        let mut settings = Notifications::default();
        assert!(!is_quiet(&settings, at(23, 0)));
        settings.quiet_hours = QuietHours {
            enabled: true,
            start: "22:00".into(),
            end: "07:00".into(),
        };
        assert!(is_quiet(&settings, at(23, 0)));
        assert!(!is_quiet(&settings, at(12, 0)));
        settings.muted = true;
        assert!(is_quiet(&settings, at(12, 0)));
    }
}
