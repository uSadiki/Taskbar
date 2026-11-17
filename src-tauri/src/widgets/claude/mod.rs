//! Claude Usage: the real 5-hour and weekly limits from the status line
//! bridge, plus a breakdown from Claude Code's local logs.

pub mod statusline;
pub mod usage;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use self::statusline::{Bridge, Limits};
use self::usage::{Block, DayTotals, ModelTotals, Totals, UsageEntry};
use crate::alerts::{self, Alert};
use crate::net;
use crate::settings::SettingsStore;

/// How far back the logs are read.
const HISTORY_DAYS: i64 = 8;
/// Thresholds that raise an alert, in percent.
const THRESHOLDS: [u8; 2] = [80, 95];
const SENT_CACHE: &str = "claude-alerts";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeState {
    /// The real limits, if the bridge has reported them.
    pub limits: Option<Limits>,
    pub bridge: Option<Bridge>,
    /// At the current pace, when the 5-hour window runs out (Unix seconds).
    pub five_hour_runs_out_at: Option<i64>,
    pub block: Option<Block>,
    pub days: Vec<DayTotals>,
    pub models: Vec<ModelTotals>,
    pub week: Totals,
    /// Claude Code's logs exist on this PC.
    pub has_logs: bool,
}

/// What was last read from each log file, so only changed files are re-read.
struct FileCache {
    modified: SystemTime,
    len: u64,
    entries: Vec<UsageEntry>,
}

#[derive(Default)]
struct Inner {
    files: HashMap<PathBuf, FileCache>,
    state: ClaudeState,
    watcher: Option<RecommendedWatcher>,
    /// Alerts already shown, as "window:resets_at:threshold".
    sent: HashSet<String>,
}

static INNER: OnceLock<Mutex<Inner>> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();
static REFRESH_QUEUED: AtomicBool = AtomicBool::new(false);

fn inner() -> &'static Mutex<Inner> {
    INNER.get_or_init(|| Mutex::new(Inner::default()))
}

pub fn state() -> ClaudeState {
    inner().lock().state.clone()
}

fn projects_dir() -> Option<PathBuf> {
    let dir = statusline::claude_dir()?.join("projects");
    dir.is_dir().then_some(dir)
}

fn log_files(dir: &Path, cutoff: SystemTime) -> Vec<(PathBuf, SystemTime, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "jsonl") {
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                // Files untouched since before the window hold nothing new.
                if modified >= cutoff {
                    out.push((path, modified, meta.len()));
                }
            }
        }
    }
    out
}

fn read_file(path: &Path, since: i64) -> Vec<UsageEntry> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| usage::parse_line(&line))
        .filter(|e| e.ts >= since)
        .collect()
}

fn compute(entries: &[UsageEntry], limits: Option<Limits>, now: i64) -> ClaudeState {
    let today = Local::now().date_naive();
    let local = |ts: i64| {
        DateTime::from_timestamp(ts, 0).map_or(today, |t| t.with_timezone(&Local).date_naive())
    };
    let (days, models) = usage::summarize(entries, today, 7, local);
    let mut week = Totals::default();
    for day in &days {
        week.tokens += day.totals.tokens;
        week.output += day.totals.output;
        week.cost += day.totals.cost;
        week.requests += day.totals.requests;
    }
    let five_hour_runs_out_at = limits.as_ref().and_then(|l| l.five_hour).and_then(|w| {
        usage::time_to_limit(w.used_percentage, w.resets_at - 5 * 3600, w.resets_at, now)
    });
    ClaudeState {
        limits,
        bridge: None,
        five_hour_runs_out_at,
        block: usage::current_block(entries, now),
        days,
        models,
        week,
        has_logs: projects_dir().is_some(),
    }
}

fn refresh() {
    let now = net::now_unix() as i64;
    let since = now - HISTORY_DAYS * 86_400;
    let cutoff = SystemTime::UNIX_EPOCH + Duration::from_secs(since.max(0) as u64);

    let files = projects_dir()
        .map(|d| log_files(&d, cutoff))
        .unwrap_or_default();
    let entries = {
        let mut inner = inner().lock();
        inner
            .files
            .retain(|path, _| files.iter().any(|(p, _, _)| p == path));
        for (path, modified, len) in &files {
            let fresh = inner
                .files
                .get(path)
                .is_some_and(|c| c.modified == *modified && c.len == *len);
            if !fresh {
                let entries = read_file(path, since);
                inner.files.insert(
                    path.clone(),
                    FileCache {
                        modified: *modified,
                        len: *len,
                        entries,
                    },
                );
            }
        }
        usage::dedupe(
            inner
                .files
                .values()
                .flat_map(|c| c.entries.iter().filter(|e| e.ts >= since).cloned()),
        )
    };

    let limits = statusline::read_limits().map(|mut l| {
        // Drop windows that have reset since Claude Code last reported.
        l.five_hour = l.five_hour.filter(|w| w.resets_at > now);
        l.seven_day = l.seven_day.filter(|w| w.resets_at > now);
        l
    });
    let mut next = compute(&entries, limits, now);

    let changed = {
        let mut inner = inner().lock();
        next.bridge = inner.state.bridge;
        let changed = inner.state != next;
        inner.state = next.clone();
        changed
    };
    if let Some(app) = APP.get() {
        if changed {
            let _ = app.emit("claude://changed", next.clone());
        }
        check_alerts(app, &next);
    }
}

/// Alert once per window and threshold: 80% and 95% of the 5-hour and the
/// weekly limit. Remembered across restarts by the window's reset time.
fn check_alerts(app: &AppHandle, state: &ClaudeState) {
    if !app
        .state::<SettingsStore>()
        .get()
        .notifications
        .claude_limits
    {
        return;
    }
    let Some(limits) = &state.limits else { return };
    let windows = [
        ("5-hour", "fiveHour", limits.five_hour),
        ("weekly", "sevenDay", limits.seven_day),
    ];
    for (label, key, window) in windows {
        let Some(window) = window else { continue };
        // Only the highest threshold crossed; 95% supersedes 80%.
        let Some(threshold) = THRESHOLDS
            .iter()
            .rev()
            .find(|t| window.used_percentage >= f64::from(**t))
        else {
            continue;
        };
        let id = format!("{key}:{}:{threshold}", window.resets_at);
        if inner().lock().sent.contains(&id) {
            continue;
        }
        let resets = DateTime::from_timestamp(window.resets_at, 0)
            .map(|t| t.with_timezone(&Local).format("%a %H:%M").to_string())
            .unwrap_or_default();
        let alert = Alert {
            id: format!("claude-{key}"),
            title: format!("Claude {label} limit at {:.0}%", window.used_percentage),
            body: format!("Resets {resets}."),
            level: if *threshold >= 95 {
                "critical"
            } else {
                "warning"
            }
            .into(),
            open: Some("claude".into()),
        };
        if alerts::show(app, alert) {
            let mut inner = inner().lock();
            inner.sent.insert(id);
            // Forget windows that have reset; keep the file small.
            let now = net::now_unix() as i64;
            inner.sent.retain(|k| {
                k.split(':')
                    .nth(1)
                    .and_then(|t| t.parse::<i64>().ok())
                    .is_some_and(|t| t > now)
            });
            net::cache_write(SENT_CACHE, &inner.sent);
        }
    }
}

pub fn request_refresh() {
    if REFRESH_QUEUED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        // Claude Code writes a burst of lines per response; read once after it.
        std::thread::sleep(Duration::from_millis(800));
        REFRESH_QUEUED.store(false, Ordering::SeqCst);
        refresh();
    });
}

/// Apply the Settings toggle to Claude Code's status line setting.
pub fn sync_bridge(enabled: bool) {
    std::thread::spawn(move || {
        let bridge = statusline::sync(enabled);
        let next = {
            let mut inner = inner().lock();
            inner.state.bridge = Some(bridge);
            inner.state.clone()
        };
        if let Some(app) = APP.get() {
            let _ = app.emit("claude://changed", next);
        }
    });
}

pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    if let Some(sent) = net::cache_read::<HashSet<String>>(SENT_CACHE) {
        inner().lock().sent = sent;
    }
    sync_bridge(app.state::<SettingsStore>().get().claude.statusline_bridge);

    // Watch the logs and the limits file; both change only while Claude Code
    // is being used, so this costs nothing at idle.
    let mut watched: Vec<PathBuf> = projects_dir().into_iter().collect();
    if let Some(dir) = statusline::limits_path().parent() {
        let _ = std::fs::create_dir_all(dir);
        watched.push(dir.to_path_buf());
    }
    let watcher = notify::recommended_watcher(|event: notify::Result<notify::Event>| {
        let relevant = event.is_ok_and(|e| {
            e.paths.iter().any(|p| {
                p.extension().is_some_and(|x| x == "jsonl")
                    || p.file_name().is_some_and(|n| n == "claude-limits.json")
            })
        });
        if relevant {
            request_refresh();
        }
    });
    match watcher {
        Ok(mut watcher) => {
            for (i, dir) in watched.iter().enumerate() {
                let mode = if i == 0 && projects_dir().as_ref() == Some(dir) {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                if let Err(e) = watcher.watch(dir, mode) {
                    log::warn!("claude: cannot watch {}: {e}", dir.display());
                }
            }
            inner().lock().watcher = Some(watcher);
        }
        Err(e) => log::warn!("claude: cannot watch for changes: {e}"),
    }

    // Windows and blocks expire with time even when nothing is written.
    tauri::async_runtime::spawn(async {
        loop {
            request_refresh();
            tokio::time::sleep(Duration::from_secs(5 * 60)).await;
        }
    });
}
