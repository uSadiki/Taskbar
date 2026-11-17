//! Token usage from Claude Code's local session logs
//! (`~/.claude/projects/**/*.jsonl`), and what can be worked out from it:
//! per-day and per-model totals, an estimated cost, the current 5-hour block
//! and how fast it is being used. Pure functions; the file reading is in
//! `mod.rs`.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BLOCK_SECONDS: i64 = 5 * 3600;

/// One API response as logged by Claude Code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageEntry {
    /// Unix seconds.
    pub ts: i64,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    pub fast: bool,
    /// `message.id` + `requestId`: the same response is logged more than once.
    pub key: String,
}

impl UsageEntry {
    pub fn tokens(&self) -> u64 {
        self.input + self.output + self.cache_write_5m + self.cache_write_1h + self.cache_read
    }
}

/// Read one log line. Anything that isn't an assistant response with usage
/// is skipped.
pub fn parse_line(line: &str) -> Option<UsageEntry> {
    // Most lines are something else; skip them without a full parse.
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return None;
    }
    let json: Value = serde_json::from_str(line).ok()?;
    if json["type"].as_str() != Some("assistant") {
        return None;
    }
    let message = &json["message"];
    let usage = &message["usage"];
    let model = message["model"].as_str()?;
    if model.starts_with('<') {
        // "<synthetic>": messages Claude Code made up locally.
        return None;
    }
    let number = |v: &Value| v.as_u64().unwrap_or(0);
    let total_write = number(&usage["cache_creation_input_tokens"]);
    let write_1h = number(&usage["cache_creation"]["ephemeral_1h_input_tokens"]);
    let write_5m = usage["cache_creation"]["ephemeral_5m_input_tokens"]
        .as_u64()
        .unwrap_or(total_write.saturating_sub(write_1h));
    let ts = DateTime::parse_from_rfc3339(json["timestamp"].as_str()?)
        .ok()?
        .timestamp();
    let id = message["id"].as_str().unwrap_or_default();
    let request = json["requestId"].as_str().unwrap_or_default();
    Some(UsageEntry {
        ts,
        model: model.to_string(),
        input: number(&usage["input_tokens"]),
        output: number(&usage["output_tokens"]),
        cache_write_5m: write_5m,
        cache_write_1h: write_1h,
        cache_read: number(&usage["cache_read_input_tokens"]),
        fast: usage["speed"].as_str() == Some("fast"),
        key: format!("{id}:{request}"),
    })
}

/// Keep one entry per response. When a response was logged several times,
/// the copy with the most output wins (later copies can be more complete).
pub fn dedupe(entries: impl IntoIterator<Item = UsageEntry>) -> Vec<UsageEntry> {
    let mut by_key: HashMap<String, UsageEntry> = HashMap::new();
    let mut anonymous = Vec::new();
    for entry in entries {
        if entry.key == ":" {
            anonymous.push(entry);
            continue;
        }
        match by_key.get(&entry.key) {
            Some(existing) if existing.output >= entry.output => {}
            _ => {
                by_key.insert(entry.key.clone(), entry);
            }
        }
    }
    let mut all: Vec<UsageEntry> = by_key.into_values().chain(anonymous).collect();
    all.sort_by_key(|e| e.ts);
    all
}

/// API list prices per million tokens: input, output, cache read.
/// Cache writes are priced from input: 1.25x for 5 minutes, 2x for an hour.
/// This is an estimate of what the usage would cost on the API; a
/// subscription is not billed this way.
fn price(model: &str) -> Option<(f64, f64, f64)> {
    let m = model.to_ascii_lowercase();
    let table: &[(&str, (f64, f64, f64))] = &[
        ("fable-5-2", (8.0, 40.0, 0.20)),
        ("mythos-5-2", (8.0, 40.0, 0.20)),
        ("fable-5", (10.0, 50.0, 1.0)),
        ("mythos-5", (10.0, 50.0, 1.0)),
        ("opus-5-5", (4.0, 20.0, 0.20)),
        ("opus-5", (5.0, 25.0, 0.50)),
        ("opus-4", (5.0, 25.0, 0.50)),
        ("sonnet-5-5", (2.0, 10.0, 0.10)),
        ("sonnet-5", (2.0, 10.0, 0.20)),
        ("sonnet-4", (3.0, 15.0, 0.30)),
        ("haiku-5-5", (0.10, 0.50, 0.01)),
        ("haiku-4", (1.0, 5.0, 0.10)),
    ];
    // Order matters: "fable-5-2" before "fable-5", and so on.
    table
        .iter()
        .find(|(name, _)| m.contains(name))
        .map(|(_, p)| *p)
}

/// Estimated API cost of one response in US dollars, or `None` for a model
/// without a known price.
pub fn cost(entry: &UsageEntry) -> Option<f64> {
    let (input, output, read) = price(&entry.model)?;
    let per = |tokens: u64, rate: f64| tokens as f64 * rate / 1_000_000.0;
    let total = per(entry.input, input)
        + per(entry.output, output)
        + per(entry.cache_write_5m, input * 1.25)
        + per(entry.cache_write_1h, input * 2.0)
        + per(entry.cache_read, read);
    // Fast mode is billed at twice the standard rate.
    Some(if entry.fast { total * 2.0 } else { total })
}

/// "claude-opus-5-5" -> "Opus 5.5".
pub fn model_label(model: &str) -> String {
    let rest = model.trim_start_matches("claude-");
    let mut parts = rest.split('-');
    let Some(family) = parts.next() else {
        return model.to_string();
    };
    let version: Vec<&str> = parts
        .take_while(|p| p.chars().all(|c| c.is_ascii_digit()) && p.len() <= 2)
        .collect();
    let mut name = family.to_string();
    if let Some(first) = name.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    if version.is_empty() {
        name
    } else {
        format!("{name} {}", version.join("."))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub tokens: u64,
    pub output: u64,
    /// Estimated, in US dollars.
    pub cost: f64,
    pub requests: u32,
}

impl Totals {
    fn add(&mut self, entry: &UsageEntry) {
        self.tokens += entry.tokens();
        self.output += entry.output;
        self.cost += cost(entry).unwrap_or(0.0);
        self.requests += 1;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayTotals {
    pub date: String,
    #[serde(flatten)]
    pub totals: Totals,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTotals {
    pub model: String,
    pub label: String,
    #[serde(flatten)]
    pub totals: Totals,
}

/// A 5-hour block, the way Claude's session limits count time: it starts at
/// the hour of the first message after a quiet spell and lasts five hours.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub start: i64,
    pub end: i64,
    #[serde(flatten)]
    pub totals: Totals,
    /// Over the time since the block's first message.
    pub tokens_per_minute: f64,
    pub cost_per_hour: f64,
    /// Where cost lands at the end of the block if the pace holds.
    pub projected_cost: f64,
}

/// The block containing `now`, if one is active.
pub fn current_block(entries: &[UsageEntry], now: i64) -> Option<Block> {
    let mut start: Option<i64> = None;
    let mut first = 0;
    let mut last = i64::MIN;
    let mut totals = Totals::default();
    for entry in entries {
        let fresh = match start {
            None => true,
            Some(s) => entry.ts >= s + BLOCK_SECONDS || entry.ts - last >= BLOCK_SECONDS,
        };
        if fresh {
            start = Some(entry.ts - entry.ts.rem_euclid(3600));
            first = entry.ts;
            totals = Totals::default();
        }
        totals.add(entry);
        last = entry.ts;
    }
    let start = start?;
    let end = start + BLOCK_SECONDS;
    if now >= end || now < start {
        return None;
    }
    let minutes = ((now - first) as f64 / 60.0).max(1.0);
    let tokens_per_minute = totals.tokens as f64 / minutes;
    let cost_per_hour = totals.cost / minutes * 60.0;
    let projected_cost = totals.cost + cost_per_hour * ((end - now) as f64 / 3600.0);
    Some(Block {
        start,
        end,
        totals,
        tokens_per_minute,
        cost_per_hour,
        projected_cost,
    })
}

/// Per-day totals for the `days` days ending on `today`, oldest first, and
/// per-model totals over the same days, largest cost first.
pub fn summarize(
    entries: &[UsageEntry],
    today: NaiveDate,
    days: u32,
    local_date: impl Fn(i64) -> NaiveDate,
) -> (Vec<DayTotals>, Vec<ModelTotals>) {
    let first = today - chrono::Days::new(u64::from(days.saturating_sub(1)));
    let mut by_day: BTreeMap<NaiveDate, Totals> = BTreeMap::new();
    let mut date = first;
    while date <= today {
        by_day.insert(date, Totals::default());
        date = date.succ_opt().unwrap_or(today);
        if date == today && by_day.contains_key(&today) {
            break;
        }
    }
    let mut by_model: HashMap<String, Totals> = HashMap::new();
    for entry in entries {
        let date = local_date(entry.ts);
        if date < first || date > today {
            continue;
        }
        by_day.entry(date).or_default().add(entry);
        by_model.entry(entry.model.clone()).or_default().add(entry);
    }
    let days = by_day
        .into_iter()
        .map(|(date, totals)| DayTotals {
            date: date.to_string(),
            totals,
        })
        .collect();
    let mut models: Vec<ModelTotals> = by_model
        .into_iter()
        .map(|(model, totals)| ModelTotals {
            label: model_label(&model),
            model,
            totals,
        })
        .collect();
    models.sort_by(|a, b| {
        b.totals
            .cost
            .total_cmp(&a.totals.cost)
            .then(b.totals.tokens.cmp(&a.totals.tokens))
    });
    (days, models)
}

/// At the current pace, when does a limit window reach 100%? `used` is the
/// percentage used, the window spans `start..end` (Unix seconds). Returns
/// `None` when the pace doesn't reach 100% before the window resets.
pub fn time_to_limit(used: f64, start: i64, end: i64, now: i64) -> Option<i64> {
    let elapsed = (now - start) as f64;
    if used <= 0.0 || elapsed <= 60.0 || now >= end {
        return None;
    }
    if used >= 100.0 {
        return Some(now);
    }
    let rate = used / elapsed;
    let at = now + ((100.0 - used) / rate) as i64;
    (at < end).then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(id: &str, ts: &str, model: &str, usage: Value) -> String {
        json!({
            "type": "assistant", "timestamp": ts, "requestId": format!("req_{id}"),
            "message": { "id": id, "model": model, "usage": usage }
        })
        .to_string()
    }

    fn entry(ts: i64, model: &str, input: u64, output: u64) -> UsageEntry {
        UsageEntry {
            ts,
            model: model.into(),
            input,
            output,
            cache_write_5m: 0,
            cache_write_1h: 0,
            cache_read: 0,
            fast: false,
            key: format!("{ts}:{model}"),
        }
    }

    #[test]
    fn parses_an_assistant_line_as_claude_code_writes_it() {
        // Shape taken from Claude Code 2.1.325's logs.
        let text = line(
            "msg_1",
            "2026-11-14T17:03:44.812Z",
            "claude-opus-5-5",
            json!({
                "input_tokens": 2, "cache_creation_input_tokens": 15301, "cache_read_input_tokens": 31758,
                "output_tokens": 565,
                "cache_creation": { "ephemeral_1h_input_tokens": 15301, "ephemeral_5m_input_tokens": 0 },
                "speed": "standard"
            }),
        );
        let e = parse_line(&text).unwrap();
        assert_eq!(e.ts, 1_794_675_824);
        assert_eq!((e.input, e.output, e.cache_read), (2, 565, 31758));
        assert_eq!((e.cache_write_5m, e.cache_write_1h), (0, 15301));
        assert!(!e.fast);
        assert_eq!(e.key, "msg_1:req_msg_1");
        assert_eq!(e.tokens(), 2 + 565 + 15301 + 31758);
    }

    #[test]
    fn older_lines_without_the_cache_split_count_writes_as_five_minute() {
        let text = line(
            "m",
            "2026-01-01T00:00:00Z",
            "claude-sonnet-4-6",
            json!({ "input_tokens": 1, "output_tokens": 1, "cache_creation_input_tokens": 500 }),
        );
        let e = parse_line(&text).unwrap();
        assert_eq!((e.cache_write_5m, e.cache_write_1h), (500, 0));
    }

    #[test]
    fn skips_lines_that_are_not_responses() {
        assert!(parse_line(r#"{"type":"user","message":{"content":"hi"}}"#).is_none());
        assert!(parse_line("not json at all \"usage\" \"assistant\"").is_none());
        let synthetic = line(
            "m",
            "2026-01-01T00:00:00Z",
            "<synthetic>",
            json!({ "input_tokens": 0 }),
        );
        assert!(parse_line(&synthetic).is_none());
        // A user line that merely mentions the words.
        let text = json!({ "type": "user", "note": "\"usage\" \"assistant\"" }).to_string();
        assert!(parse_line(&text).is_none());
    }

    #[test]
    fn duplicates_collapse_to_the_most_complete_copy() {
        let mut a = entry(10, "claude-opus-5-5", 5, 10);
        a.key = "m1:r1".into();
        let mut b = a.clone();
        b.output = 300;
        let mut c = entry(5, "claude-opus-5-5", 1, 1);
        c.key = "m2:r2".into();
        let all = dedupe([a, b, c]);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].key, "m2:r2", "sorted by time");
        assert_eq!(all[1].output, 300);
    }

    #[test]
    fn prices_known_models_and_skips_unknown_ones() {
        let mut e = entry(0, "claude-opus-5-5", 1_000_000, 1_000_000);
        assert!((cost(&e).unwrap() - 24.0).abs() < 1e-9);
        e.cache_read = 1_000_000;
        e.cache_write_1h = 1_000_000;
        // + 0.20 read + 8.00 (2x input) write.
        assert!((cost(&e).unwrap() - 32.2).abs() < 1e-9);
        e.fast = true;
        assert!((cost(&e).unwrap() - 64.4).abs() < 1e-9);
        assert!(
            cost(&entry(0, "claude-fable-5-2", 1_000_000, 0))
                .is_some_and(|c| (c - 8.0).abs() < 1e-9)
        );
        assert!(
            cost(&entry(0, "claude-fable-5-1", 1_000_000, 0))
                .is_some_and(|c| (c - 10.0).abs() < 1e-9)
        );
        assert!(cost(&entry(0, "some-other-model", 1, 1)).is_none());
    }

    #[test]
    fn labels_models_for_display() {
        assert_eq!(model_label("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(model_label("claude-fable-5-2"), "Fable 5.2");
        assert_eq!(model_label("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(model_label("claude-sonnet-5"), "Sonnet 5");
    }

    #[test]
    fn the_current_block_starts_on_the_hour_of_its_first_message() {
        let h = 3600;
        let base = 1_800_000_000 - 1_800_000_000 % h; // on the hour
        let entries = [
            entry(base - 10 * h, "claude-opus-5-5", 1, 1), // an old block
            entry(base + 20 * 60, "claude-opus-5-5", 1000, 0),
            entry(base + 80 * 60, "claude-opus-5-5", 1000, 0),
        ];
        let now = base + 100 * 60;
        let block = current_block(&entries, now).unwrap();
        assert_eq!((block.start, block.end), (base, base + 5 * h));
        assert_eq!(block.totals.tokens, 2000);
        assert_eq!(block.totals.requests, 2);
        // 2000 tokens over the 80 minutes since the block's first message.
        assert!((block.tokens_per_minute - 25.0).abs() < 1e-9);
        assert!(block.projected_cost > block.totals.cost);
    }

    #[test]
    fn a_block_ends_after_five_hours_or_a_long_gap() {
        let h = 3600;
        let base = 1_800_000_000 - 1_800_000_000 % h;
        let entries = [entry(base, "claude-opus-5-5", 1, 1)];
        assert!(current_block(&entries, base + 5 * h).is_none());
        // A message 5h20 later starts a new block on its own hour.
        let entries = [
            entry(base, "claude-opus-5-5", 1, 1),
            entry(base + 5 * h + 20 * 60, "claude-opus-5-5", 7, 0),
        ];
        let block = current_block(&entries, base + 5 * h + 30 * 60).unwrap();
        assert_eq!(block.start, base + 5 * h);
        assert_eq!(block.totals.tokens, 7);
    }

    #[test]
    fn no_entries_no_block() {
        assert!(current_block(&[], 1_800_000_000).is_none());
    }

    #[test]
    fn summarizes_by_day_and_model() {
        let day = |ts: i64| DateTime::from_timestamp(ts, 0).unwrap().date_naive();
        let d0 = 1_794_614_400; // 2026-11-14 00:00 UTC
        let entries = [
            entry(d0 - 86_400 * 9, "claude-opus-5-5", 9, 9), // outside the window
            entry(d0 + 100, "claude-opus-5-5", 1_000_000, 0),
            entry(d0 + 86_400 + 100, "claude-haiku-5-5", 1_000_000, 0),
            entry(d0 + 86_400 + 200, "claude-opus-5-5", 1_000_000, 0),
        ];
        let today = day(d0 + 86_400 * 2);
        let (days, models) = summarize(&entries, today, 7, day);
        assert_eq!(days.len(), 7);
        assert_eq!(days.last().unwrap().date, "2026-11-16");
        assert_eq!(days[4].date, "2026-11-14");
        assert_eq!(days[4].totals.tokens, 1_000_000);
        assert_eq!(days[5].totals.requests, 2);
        assert_eq!(days[6].totals.requests, 0);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].label, "Opus 5.5", "largest cost first");
        assert_eq!(models[0].totals.requests, 2);
    }

    #[test]
    fn projects_when_a_window_runs_out() {
        let start = 1_000_000;
        let end = start + 5 * 3600;
        // 50% used in the first hour: at this pace 100% one hour later.
        assert_eq!(
            time_to_limit(50.0, start, end, start + 3600),
            Some(start + 7200)
        );
        // 10% in four hours won't reach 100% before the reset.
        assert_eq!(time_to_limit(10.0, start, end, start + 4 * 3600), None);
        assert_eq!(
            time_to_limit(100.0, start, end, start + 60 * 10),
            Some(start + 600)
        );
        assert_eq!(time_to_limit(0.0, start, end, start + 3600), None);
    }
}
