//! Settings schema and the store that owns the settings file.
//!
//! The store is the single source of truth. Windows read it through commands,
//! change it by sending a JSON merge patch, and are told about every change
//! with a `settings://changed` event.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

pub const SETTINGS_VERSION: u32 = 1;

/// Widgets the user can reorder and switch off. The clock is always last and
/// is not part of this list.
pub const WIDGET_IDS: &[&str] = &[
    "apps",
    "nowPlaying",
    "weather",
    "github",
    "claude",
    "battery",
    "toggles",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskbarMode {
    /// Set the Windows taskbar to auto-hide while the dock runs.
    #[default]
    AutoHide,
    /// Auto-hide it and also hide its window, so it never slides in.
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    #[default]
    Bottom,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DockLayoutKind {
    #[default]
    Pill,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DockSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl DockSize {
    /// Height of the dock itself, in logical pixels.
    pub const fn dock_height(self) -> f64 {
        match self {
            DockSize::Small => 44.0,
            DockSize::Medium => 52.0,
            DockSize::Large => 60.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorScheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct General {
    pub autostart: bool,
    pub taskbar_mode: TaskbarMode,
    pub position: Edge,
    pub layout: DockLayoutKind,
    /// Device name of the monitor the dock lives on (for example `\\.\DISPLAY1`).
    /// `None` means the primary monitor.
    pub monitor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Appearance {
    pub theme: String,
    pub color_scheme: ColorScheme,
    /// Accent colour as `#rrggbb`. `None` follows the Windows accent colour.
    pub accent: Option<String>,
    pub dock_size: DockSize,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "glass".into(),
            color_scheme: ColorScheme::System,
            accent: None,
            dock_size: DockSize::Medium,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetEntry {
    pub id: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QuietHours {
    pub enabled: bool,
    /// Local time, `HH:MM`.
    pub start: String,
    pub end: String,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            enabled: false,
            start: "22:00".into(),
            end: "07:00".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Notifications {
    pub quiet_hours: QuietHours,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub general: General,
    pub appearance: Appearance,
    pub widgets: Vec<WidgetEntry>,
    pub notifications: Notifications,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            general: General::default(),
            appearance: Appearance::default(),
            widgets: default_widgets(),
            notifications: Notifications::default(),
        }
    }
}

fn default_widgets() -> Vec<WidgetEntry> {
    WIDGET_IDS
        .iter()
        .map(|id| WidgetEntry {
            id: (*id).into(),
            enabled: true,
        })
        .collect()
}

impl Settings {
    /// Build settings from stored JSON of any age: migrate, fill in defaults
    /// for missing fields, then normalize.
    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        let mut settings: Settings = serde_json::from_value(migrate(value))?;
        settings.sanitize();
        Ok(settings)
    }

    /// Enforce invariants the type system doesn't: every known widget appears
    /// exactly once, unknown widgets are dropped, the version is current.
    pub fn sanitize(&mut self) {
        self.version = SETTINGS_VERSION;

        let mut seen: Vec<String> = Vec::new();
        self.widgets.retain(|w| {
            let known = WIDGET_IDS.contains(&w.id.as_str());
            let fresh = !seen.contains(&w.id);
            if known && fresh {
                seen.push(w.id.clone());
            }
            known && fresh
        });
        for id in WIDGET_IDS {
            if !seen.iter().any(|s| s == id) {
                self.widgets.push(WidgetEntry {
                    id: (*id).into(),
                    enabled: true,
                });
            }
        }

        if self
            .general
            .monitor
            .as_deref()
            .is_some_and(|m| m.trim().is_empty())
        {
            self.general.monitor = None;
        }
    }
}

/// Bring stored JSON from an older schema version up to the current one.
/// Version 1 is the first schema, so this only repairs the envelope.
pub fn migrate(value: Value) -> Value {
    let mut value = match value {
        Value::Object(map) => Value::Object(map),
        _ => Value::Object(serde_json::Map::new()),
    };
    if let Some(map) = value.as_object_mut() {
        let version = map.get("version").and_then(Value::as_u64).unwrap_or(0);
        if version == 0 {
            map.insert("version".into(), Value::from(SETTINGS_VERSION));
        }
    }
    value
}

/// JSON merge patch (RFC 7386): objects merge key by key, `null` removes a
/// key (the field then falls back to its default), anything else replaces.
pub fn merge_patch(base: &mut Value, patch: &Value) {
    match patch {
        Value::Object(patch_map) => {
            if !base.is_object() {
                *base = Value::Object(serde_json::Map::new());
            }
            if let Some(base_map) = base.as_object_mut() {
                for (key, patch_value) in patch_map {
                    if patch_value.is_null() {
                        base_map.remove(key);
                    } else {
                        merge_patch(
                            base_map.entry(key.clone()).or_insert(Value::Null),
                            patch_value,
                        );
                    }
                }
            }
        }
        other => *base = other.clone(),
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<Settings>,
}

impl SettingsStore {
    /// Load settings from `path`. A missing file yields defaults. An unreadable
    /// file is set aside as `settings.json.bad-<time>` so nothing is lost, and
    /// defaults are used.
    pub fn load(path: PathBuf) -> Self {
        let current = RwLock::new(read_file(&path));
        Self { path, current }
    }

    pub fn get(&self) -> Settings {
        self.current.read().clone()
    }

    /// Apply a merge patch. Returns the settings before and after. Nothing
    /// changes if the patched settings are invalid or cannot be saved.
    pub fn apply_patch(&self, patch: &Value) -> AppResult<(Settings, Settings)> {
        let old = self.get();
        let mut value = serde_json::to_value(&old)?;
        merge_patch(&mut value, patch);
        let mut new: Settings =
            serde_json::from_value(value).map_err(|e| AppError::InvalidSettings(e.to_string()))?;
        new.sanitize();
        write_file(&self.path, &new)?;
        *self.current.write() = new.clone();
        Ok((old, new))
    }

    /// Re-read the file from disk, for the tray's Reload.
    pub fn reload(&self) -> (Settings, Settings) {
        let old = self.get();
        let new = read_file(&self.path);
        *self.current.write() = new.clone();
        (old, new)
    }
}

fn read_file(path: &Path) -> Settings {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            log::warn!(
                "settings: cannot read {}: {e}; using defaults",
                path.display()
            );
            return Settings::default();
        }
    };
    let parsed = serde_json::from_str::<Value>(&text)
        .map_err(|e| e.to_string())
        .and_then(|v| Settings::from_value(v).map_err(|e| e.to_string()));
    match parsed {
        Ok(settings) => settings,
        Err(e) => {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let aside = path.with_extension(format!("json.bad-{stamp}"));
            log::warn!(
                "settings: {} is invalid ({e}); moved to {} and using defaults",
                path.display(),
                aside.display()
            );
            let _ = fs::rename(path, aside);
            Settings::default()
        }
    }
}

/// Write to a temporary file and rename it over the target, so a crash
/// mid-write can't leave a truncated settings file.
fn write_file(path: &Path, settings: &Settings) -> AppResult<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("glassdock-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn defaults_survive_a_round_trip() {
        let settings = Settings::default();
        let value = serde_json::to_value(&settings).unwrap();
        assert_eq!(Settings::from_value(value).unwrap(), settings);
        assert_eq!(settings.general.taskbar_mode, TaskbarMode::AutoHide);
        assert_eq!(settings.general.position, Edge::Bottom);
        assert_eq!(settings.widgets.len(), WIDGET_IDS.len());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let settings = Settings::from_value(json!({ "general": { "position": "top" } })).unwrap();
        assert_eq!(settings.general.position, Edge::Top);
        assert_eq!(settings.general.layout, DockLayoutKind::Pill);
        assert_eq!(settings.appearance.theme, "glass");
        assert_eq!(settings.version, SETTINGS_VERSION);
    }

    #[test]
    fn merge_patch_merges_nested_objects_and_null_removes() {
        let mut base = json!({ "a": { "b": 1, "c": 2 }, "d": [1, 2], "e": "x" });
        merge_patch(
            &mut base,
            &json!({ "a": { "b": 9, "c": null }, "d": [3], "f": true }),
        );
        assert_eq!(
            base,
            json!({ "a": { "b": 9 }, "d": [3], "e": "x", "f": true })
        );
    }

    #[test]
    fn widgets_are_deduplicated_filtered_and_completed() {
        let settings = Settings::from_value(json!({
            "widgets": [
                { "id": "weather", "enabled": false },
                { "id": "bogus" },
                { "id": "apps" },
                { "id": "weather", "enabled": true }
            ]
        }))
        .unwrap();
        let ids: Vec<&str> = settings.widgets.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(&ids[..2], &["weather", "apps"]);
        assert_eq!(ids.len(), WIDGET_IDS.len());
        assert!(!settings.widgets[0].enabled, "first occurrence wins");
        assert!(!ids.contains(&"bogus"));
    }

    #[test]
    fn migrate_repairs_the_envelope() {
        assert_eq!(
            migrate(json!("nonsense")),
            json!({ "version": SETTINGS_VERSION })
        );
        assert_eq!(
            migrate(json!({ "general": {} }))["version"],
            json!(SETTINGS_VERSION)
        );
    }

    #[test]
    fn store_persists_patches() {
        let path = temp_path("persist");
        let store = SettingsStore::load(path.clone());
        let (old, new) = store
            .apply_patch(
                &json!({ "general": { "taskbarMode": "hidden", "monitor": "\\\\.\\DISPLAY2" } }),
            )
            .unwrap();
        assert_eq!(old.general.taskbar_mode, TaskbarMode::AutoHide);
        assert_eq!(new.general.taskbar_mode, TaskbarMode::Hidden);

        let reloaded = SettingsStore::load(path.clone()).get();
        assert_eq!(reloaded, new);
        assert_eq!(reloaded.general.monitor.as_deref(), Some("\\\\.\\DISPLAY2"));

        // null clears an optional field back to its default.
        let (_, cleared) = store
            .apply_patch(&json!({ "general": { "monitor": null } }))
            .unwrap();
        assert_eq!(cleared.general.monitor, None);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn invalid_patch_changes_nothing() {
        let path = temp_path("invalid");
        let store = SettingsStore::load(path.clone());
        let before = store.get();
        let result = store.apply_patch(&json!({ "general": { "position": "sideways" } }));
        assert!(matches!(result, Err(AppError::InvalidSettings(_))));
        assert_eq!(store.get(), before);
        assert!(!path.exists(), "nothing is written for an invalid patch");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_is_set_aside_not_lost() {
        let path = temp_path("corrupt");
        fs::write(&path, "{ this is not json").unwrap();
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get(), Settings::default());
        assert!(!path.exists());
        let kept = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains("bad-"));
        assert!(kept, "the unreadable file is kept under a .bad-<time> name");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
