//! The bridge to Claude Code's real usage limits.
//!
//! Claude Code runs a configurable "status line" command after each response
//! and passes it, among other things, `rate_limits.five_hour` and
//! `rate_limits.seven_day` (`used_percentage`, `resets_at`). Glassdock
//! registers itself as that command (`glassdock.exe --statusline`), saves the
//! limits to a file the dock watches, and prints a short status line back.
//! This is the documented interface; nothing private is read.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub used_percentage: f64,
    /// Unix seconds.
    pub resets_at: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    /// Unix seconds when Claude Code last reported.
    pub updated_at: i64,
}

pub fn limits_path() -> PathBuf {
    paths::local_dir().join("claude-limits.json")
}

pub fn read_limits() -> Option<Limits> {
    serde_json::from_str(&fs::read_to_string(limits_path()).ok()?).ok()
}

fn window(value: &Value) -> Option<Window> {
    Some(Window {
        used_percentage: value["used_percentage"].as_f64()?,
        resets_at: value["resets_at"].as_i64()?,
    })
}

/// Merge what Claude Code just sent into what was known. A window missing
/// from the input keeps its previous value (Claude Code leaves windows out
/// before the first response and after they reset); one whose reset time has
/// passed is dropped.
pub fn merge(previous: Option<Limits>, input: &Value, now: i64) -> Option<Limits> {
    let rate = input.get("rate_limits")?;
    let mut limits = previous.unwrap_or_default();
    if let Some(w) = window(&rate["five_hour"]) {
        limits.five_hour = Some(w);
    }
    if let Some(w) = window(&rate["seven_day"]) {
        limits.seven_day = Some(w);
    }
    limits.five_hour = limits.five_hour.filter(|w| w.resets_at > now);
    limits.seven_day = limits.seven_day.filter(|w| w.resets_at > now);
    limits.updated_at = now;
    Some(limits)
}

/// The line Claude Code shows under its prompt.
pub fn status_text(input: &Value, limits: Option<&Limits>) -> String {
    let model = input["model"]["display_name"].as_str().unwrap_or("Claude");
    let mut parts = vec![model.to_string()];
    if let Some(context) = input["context_window"]["used_percentage"].as_f64() {
        parts.push(format!("context {context:.0}%"));
    }
    if let Some(l) = limits {
        if let Some(w) = l.five_hour {
            parts.push(format!("5h {:.0}%", w.used_percentage));
        }
        if let Some(w) = l.seven_day {
            parts.push(format!("week {:.0}%", w.used_percentage));
        }
    }
    parts.join(" | ")
}

/// `glassdock.exe --statusline`: read Claude Code's JSON from stdin, save the
/// limits, print the status line. Never fails loudly: Claude Code shows
/// whatever is printed, so a problem prints a plain fallback.
pub fn run() {
    let mut text = String::new();
    let _ = std::io::stdin()
        .take(4 * 1024 * 1024)
        .read_to_string(&mut text);
    // Some shells put a byte-order mark in front of piped text.
    let input: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or(Value::Null);
    let now = crate::net::now_unix() as i64;
    let limits = merge(read_limits(), &input, now);
    if let Some(limits) = &limits
        && let Ok(json) = serde_json::to_vec(limits)
    {
        let path = limits_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
    let shown = limits.or_else(read_limits);
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", status_text(&input, shown.as_ref()));
    let _ = out.flush();
}

// ---------------------------------------------------------------------------
// Registering with Claude Code
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Bridge {
    /// Glassdock is Claude Code's status line command.
    Installed,
    /// Another status line is configured; it is left alone.
    OtherCommand,
    /// Turned off in Settings.
    Off,
    /// A development build; it doesn't register itself (see `sync`).
    DevBuild,
    /// No Claude Code settings folder on this PC.
    NoClaudeCode,
    /// The settings file could not be read or written.
    Error,
}

pub fn claude_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let dir = home.join(".claude");
    dir.is_dir().then_some(dir)
}

/// The command Claude Code should run. Forward slashes work in both the Git
/// Bash and cmd shells Claude Code may use on Windows.
pub fn command_for(exe: &Path) -> String {
    format!(
        "\"{}\" --statusline",
        exe.display().to_string().replace('\\', "/")
    )
}

fn is_ours(command: &str) -> bool {
    command.to_ascii_lowercase().contains("glassdock") && command.contains("--statusline")
}

/// Change Claude Code's settings so the status line is ours (`install`) or
/// isn't (`!install`). Returns the new settings, or `None` if nothing needs
/// to change, plus the resulting state. A status line set up by something
/// else is never replaced or removed.
pub fn apply(settings: &Value, command: &str, install: bool) -> (Option<Value>, Bridge) {
    let mut object: Map<String, Value> = settings.as_object().cloned().unwrap_or_default();
    let current = object
        .get("statusLine")
        .and_then(|s| s["command"].as_str())
        .map(str::to_string);
    let ours = current.as_deref().is_some_and(is_ours);
    if current.is_some() && !ours {
        return (
            None,
            if install {
                Bridge::OtherCommand
            } else {
                Bridge::Off
            },
        );
    }
    if install {
        if current.as_deref() == Some(command) {
            return (None, Bridge::Installed);
        }
        object.insert(
            "statusLine".into(),
            json!({ "type": "command", "command": command }),
        );
        (Some(Value::Object(object)), Bridge::Installed)
    } else if ours {
        object.remove("statusLine");
        (Some(Value::Object(object)), Bridge::Off)
    } else {
        (None, Bridge::Off)
    }
}

/// Make Claude Code's settings match the Settings toggle. Development builds
/// only ever remove the entry, so a dev run never points Claude Code at a
/// build-folder executable that will disappear.
pub fn sync(enabled: bool) -> Bridge {
    let Some(dir) = claude_dir() else {
        return Bridge::NoClaudeCode;
    };
    let Ok(exe) = std::env::current_exe() else {
        return Bridge::Error;
    };
    let dev = cfg!(debug_assertions);
    let path = dir.join("settings.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".into(),
        Err(_) => return Bridge::Error,
    };
    let Ok(settings) = serde_json::from_str::<Value>(&text) else {
        // Someone else's file in a state we don't understand: hands off.
        return Bridge::Error;
    };
    let (changed, state) = apply(&settings, &command_for(&exe), enabled && !dev);
    if let Some(next) = changed {
        let Ok(mut json) = serde_json::to_string_pretty(&next) else {
            return Bridge::Error;
        };
        json.push('\n');
        let tmp = path.with_extension("json.glassdock-tmp");
        if fs::write(&tmp, json).is_err() || fs::rename(&tmp, &path).is_err() {
            let _ = fs::remove_file(&tmp);
            return Bridge::Error;
        }
        log::info!("claude: status line bridge is now {state:?}");
    }
    match state {
        Bridge::Off if enabled && dev => Bridge::DevBuild,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "\"C:/Users/u/AppData/Local/Glassdock/glassdock.exe\" --statusline";

    fn input(five: f64, week: f64) -> Value {
        json!({
            "model": { "display_name": "Opus 5.5" },
            "context_window": { "used_percentage": 8 },
            "rate_limits": {
                "five_hour": { "used_percentage": five, "resets_at": 2_000 },
                "seven_day": { "used_percentage": week, "resets_at": 9_000 }
            }
        })
    }

    #[test]
    fn reads_both_windows() {
        let limits = merge(None, &input(23.5, 41.2), 1_000).unwrap();
        assert_eq!(
            limits.five_hour,
            Some(Window {
                used_percentage: 23.5,
                resets_at: 2_000
            })
        );
        assert_eq!(limits.seven_day.unwrap().used_percentage, 41.2);
        assert_eq!(limits.updated_at, 1_000);
    }

    #[test]
    fn input_without_rate_limits_changes_nothing() {
        assert!(merge(None, &json!({ "model": {} }), 1_000).is_none());
        assert!(merge(None, &Value::Null, 1_000).is_none());
    }

    #[test]
    fn a_missing_window_keeps_its_last_value_until_it_resets() {
        let previous = merge(None, &input(23.5, 41.2), 1_000);
        let only_week = json!({ "rate_limits": { "seven_day": { "used_percentage": 42.0, "resets_at": 9_000 } } });
        let limits = merge(previous.clone(), &only_week, 1_500).unwrap();
        assert_eq!(limits.five_hour.unwrap().used_percentage, 23.5);
        assert_eq!(limits.seven_day.unwrap().used_percentage, 42.0);
        // After the five-hour reset time it is dropped.
        let limits = merge(previous, &only_week, 2_500).unwrap();
        assert!(limits.five_hour.is_none());
    }

    #[test]
    fn status_text_shows_what_is_known() {
        let limits = merge(None, &input(23.5, 41.2), 1_000);
        assert_eq!(
            status_text(&input(23.5, 41.2), limits.as_ref()),
            "Opus 5.5 | context 8% | 5h 24% | week 41%"
        );
        assert_eq!(status_text(&Value::Null, None), "Claude");
    }

    #[test]
    fn installs_into_empty_or_existing_settings_and_keeps_other_keys() {
        let (next, state) = apply(&json!({ "model": "opus", "theme": "dark" }), CMD, true);
        let next = next.unwrap();
        assert_eq!(state, Bridge::Installed);
        assert_eq!(next["statusLine"]["command"], CMD);
        assert_eq!(next["statusLine"]["type"], "command");
        assert_eq!(next["model"], "opus");
        assert_eq!(next["theme"], "dark");
        assert!(apply(&json!({}), CMD, true).0.is_some());
    }

    #[test]
    fn already_installed_is_a_no_op_and_a_moved_exe_is_updated() {
        let settings = json!({ "statusLine": { "type": "command", "command": CMD } });
        assert_eq!(apply(&settings, CMD, true), (None, Bridge::Installed));
        let moved = "\"D:/Apps/Glassdock/glassdock.exe\" --statusline";
        let (next, _) = apply(&settings, moved, true);
        assert_eq!(next.unwrap()["statusLine"]["command"], moved);
    }

    #[test]
    fn someone_elses_status_line_is_never_touched() {
        let settings =
            json!({ "statusLine": { "type": "command", "command": "~/.claude/my-line.sh" } });
        assert_eq!(apply(&settings, CMD, true), (None, Bridge::OtherCommand));
        assert_eq!(apply(&settings, CMD, false), (None, Bridge::Off));
    }

    #[test]
    fn uninstall_removes_only_our_entry() {
        let settings =
            json!({ "statusLine": { "type": "command", "command": CMD }, "model": "opus" });
        let (next, state) = apply(&settings, CMD, false);
        let next = next.unwrap();
        assert_eq!(state, Bridge::Off);
        assert!(next.get("statusLine").is_none());
        assert_eq!(next["model"], "opus");
        assert_eq!(apply(&json!({}), CMD, false), (None, Bridge::Off));
    }

    #[test]
    fn command_uses_forward_slashes_and_quotes() {
        let cmd = command_for(Path::new(r"C:\Program Files\Glassdock\glassdock.exe"));
        assert_eq!(
            cmd,
            "\"C:/Program Files/Glassdock/glassdock.exe\" --statusline"
        );
        assert!(is_ours(&cmd));
    }
}
