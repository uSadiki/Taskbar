//! Brightness of the built-in display, through WMI (`root\WMI`).
//!
//! External monitors don't appear here; they would need DDC/CI. Must be
//! called on the shell worker thread: a WMI connection can't move between
//! threads, so one is kept there.

use std::cell::RefCell;
use std::collections::HashMap;

use serde::Deserialize;
use wmi::{Variant, WMIConnection};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MonitorBrightness {
    current_brightness: u8,
    instance_name: String,
}

thread_local! {
    static CONNECTION: RefCell<Option<WMIConnection>> = const { RefCell::new(None) };
}

fn with_connection<T>(action: impl FnOnce(&WMIConnection) -> Option<T>) -> Option<T> {
    CONNECTION.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = WMIConnection::with_namespace_path(r"ROOT\WMI").ok();
        }
        slot.as_ref().and_then(action)
    })
}

fn monitors(connection: &WMIConnection) -> Option<Vec<MonitorBrightness>> {
    connection
        .raw_query(
            "SELECT CurrentBrightness, InstanceName FROM WmiMonitorBrightness WHERE Active = TRUE",
        )
        .ok()
}

/// Current brightness, 0 to 100. `None` when no display supports it (a
/// desktop PC, or an external monitor only).
pub fn get() -> Option<u8> {
    with_connection(|connection| {
        monitors(connection)?
            .first()
            .map(|m| m.current_brightness.min(100))
    })
}

/// WMI object paths quote the instance name, with backslashes doubled.
fn method_path(instance_name: &str) -> String {
    format!(
        "WmiMonitorBrightnessMethods.InstanceName=\"{}\"",
        instance_name.replace('\\', "\\\\")
    )
}

pub fn set(percent: u8) -> bool {
    with_connection(|connection| {
        let instance = monitors(connection)?.into_iter().next()?.instance_name;
        // Timeout 0: apply immediately.
        let params: HashMap<String, Variant> = HashMap::from([
            ("Timeout".to_string(), Variant::UI4(0)),
            ("Brightness".to_string(), Variant::UI1(percent.min(100))),
        ]);
        let input = connection
            .get_object("WmiMonitorBrightnessMethods")
            .ok()?
            .get_method("WmiSetBrightness")
            .ok()??
            .spawn_instance()
            .ok()?;
        for (name, value) in params {
            input.put_property(&name, value).ok()?;
        }
        connection
            .exec_method(method_path(&instance), "WmiSetBrightness", Some(&input))
            .ok()?;
        Some(())
    })
    .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_path_escapes_backslashes() {
        assert_eq!(
            method_path(r"DISPLAY\SDC41A3\4&36409b60&0&UID8388688_0"),
            r#"WmiMonitorBrightnessMethods.InstanceName="DISPLAY\\SDC41A3\\4&36409b60&0&UID8388688_0""#
        );
    }
}
