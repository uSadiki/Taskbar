//! Claude Usage: the real 5-hour and weekly limits from the status line
//! bridge, plus a breakdown from Claude Code's local logs.

pub mod statusline;
pub mod usage;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
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
    /// Bytes read so far, up to the end of the last complete line. Claude
    /// Code only appends, so the next read starts here.
    offset: u64,
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

/// Read complete lines from `offset` on. Returns the entries and the offset
/// just past the last complete line; a line still being written is left for
/// the next read.
fn read_from(path: &Path, offset: u64, since: i64) -> (Vec<UsageEntry>, u64) {
    let Ok(mut file) = File::open(path) else {
        return (Vec::new(), offset);
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return (Vec::new(), offset);
    }
    let mut reader = BufReader::new(file);
    let mut entries = Vec::new();
    let mut consumed = offset;
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if !line.ends_with('\n') {
                    break;
                }
                consumed += n as u64;
                if let Some(entry) = usage::parse_line(line.trim_end())
                    && entry.ts >= since
                {
                    entries.push(entry);
                }
            }
        }
    }
    (entries, consumed)
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
            if fresh {
                continue;
            }
            match inner.files.get_mut(path) {
                // Grown: read only what was appended.
                Some(cache) if *len >= cache.len => {
                    let (more, offset) = read_from(path, cache.offset, since);
                    cache.entries.extend(more);
                    cache.offset = offset;
                    cache.modified = *modified;
                    cache.len = *len;
                }
                // New, or rewritten from scratch.
                _ => {
                    let (entries, offset) = read_from(path, 0, since);
                    inner.files.insert(
                        path.clone(),
                        FileCache {
                            modified: *modified,
                            len: *len,
                            offset,
                            entries,
                        },
                    );
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(id: &str, ts: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","requestId":"r{id}","message":{{"id":"{id}","model":"claude-opus-5-5","usage":{{"input_tokens":1,"output_tokens":2}}}}}}"#
        )
    }

    #[test]
    fn reads_only_appended_complete_lines() {
        let dir = std::env::temp_dir().join(format!("glassdock-claude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut file = File::create(&path).unwrap();
        writeln!(file, "{}", line("a", "2026-11-17T10:00:00Z")).unwrap();
        write!(file, "{}", &line("b", "2026-11-17T10:01:00Z")[..40]).unwrap();
        file.flush().unwrap();

        let (first, offset) = read_from(&path, 0, 0);
        assert_eq!(
            first.len(),
            1,
            "the half-written second line is left for later"
        );

        // Finish the second line and add a third.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{}", line("a", "2026-11-17T10:00:00Z")).unwrap();
        writeln!(file, "{}", line("b", "2026-11-17T10:01:00Z")).unwrap();
        writeln!(file, "{}", line("c", "2026-11-17T10:02:00Z")).unwrap();
        file.flush().unwrap();

        let (more, end) = read_from(&path, offset, 0);
        assert_eq!(
            more.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
            ["b:rb", "c:rc"]
        );
        assert_eq!(end, std::fs::metadata(&path).unwrap().len());
        let (nothing, same) = read_from(&path, end, 0);
        assert!(nothing.is_empty());
        assert_eq!(same, end);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
