//! GitHub Activity: the contribution calendar, streaks, recent events, open
//! pull requests and review requests, for the account the token belongs to.
//!
//! The token is kept in Windows Credential Manager, never in the settings
//! file, and never leaves the Rust side. Refreshes every 15 minutes; the last
//! good answer is cached on disk for startup and for offline use.

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Local, NaiveDate};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

use crate::net;

const REFRESH: Duration = Duration::from_secs(15 * 60);
const CACHE: &str = "github";
const KEYRING_SERVICE: &str = "Glassdock";
const KEYRING_USER: &str = "github-token";

const QUERY: &str = r#"
query {
  viewer {
    login
    contributionsCollection {
      contributionCalendar {
        totalContributions
        weeks { contributionDays { date contributionCount contributionLevel } }
      }
    }
  }
  authored: search(query: "is:pr is:open author:@me archived:false sort:updated-desc", type: ISSUE, first: 10) {
    issueCount
    nodes { ...pr }
  }
  reviews: search(query: "is:pr is:open review-requested:@me archived:false sort:updated-desc", type: ISSUE, first: 10) {
    issueCount
    nodes { ...pr }
  }
  rateLimit { remaining resetAt }
}
fragment pr on PullRequest {
  title url number isDraft updatedAt
  repository { nameWithOwner }
}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    NoToken,
    Ok,
    InvalidToken,
    RateLimited,
    Offline,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributionDay {
    pub date: String,
    pub count: u32,
    /// 0 (none) to 4 (most), as GitHub draws it.
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub title: String,
    pub url: String,
    pub number: u64,
    pub repo: String,
    pub draft: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// "push", "pr", "issue", "review", "comment", "create" or "other".
    pub kind: String,
    /// What happened, in a few words: "Pushed 3 commits", "Opened #12".
    pub summary: String,
    pub repo: String,
    pub url: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubData {
    pub login: String,
    pub total_year: u32,
    pub days: Vec<ContributionDay>,
    pub today: u32,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub authored: Vec<PullRequest>,
    pub authored_total: u64,
    pub review_requests: Vec<PullRequest>,
    pub review_total: u64,
    pub events: Vec<Event>,
    pub fetched_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubState {
    pub status: Status,
    /// Details for the status, when there is something to say.
    pub message: Option<String>,
    /// The last good data; kept through errors.
    pub data: Option<GithubData>,
}

impl Default for GithubState {
    fn default() -> Self {
        Self {
            status: Status::NoToken,
            message: None,
            data: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure parsing and arithmetic
// ---------------------------------------------------------------------------

fn level_number(level: &str) -> u8 {
    match level {
        "FIRST_QUARTILE" => 1,
        "SECOND_QUARTILE" => 2,
        "THIRD_QUARTILE" => 3,
        "FOURTH_QUARTILE" => 4,
        _ => 0,
    }
}

/// Today's count, the current streak and the longest streak. A streak is a
/// run of days with at least one contribution. Today without contributions
/// (yet) doesn't break the current streak; it just isn't part of it.
pub fn streaks(days: &[ContributionDay], today: NaiveDate) -> (u32, u32, u32) {
    let parsed: Vec<(NaiveDate, u32)> = days
        .iter()
        .filter_map(|d| {
            Some((
                NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok()?,
                d.count,
            ))
        })
        .filter(|(date, _)| *date <= today)
        .collect();

    let mut longest = 0;
    let mut run = 0;
    let mut previous: Option<NaiveDate> = None;
    for (date, count) in &parsed {
        let consecutive = previous.is_some_and(|p| p.succ_opt() == Some(*date));
        if *count > 0 {
            run = if consecutive || previous.is_none() {
                run + 1
            } else {
                1
            };
        } else {
            run = 0;
        }
        longest = longest.max(run);
        previous = Some(*date);
    }

    let today_count = parsed
        .iter()
        .find(|(d, _)| *d == today)
        .map_or(0, |(_, c)| *c);
    let mut current = 0;
    let mut expected = if today_count > 0 {
        Some(today)
    } else {
        today.pred_opt()
    };
    for (date, count) in parsed.iter().rev() {
        if Some(*date) > expected {
            continue;
        }
        if Some(*date) != expected || *count == 0 {
            break;
        }
        current += 1;
        expected = date.pred_opt();
    }
    (today_count, current, longest)
}

fn parse_prs(search: &Value) -> (Vec<PullRequest>, u64) {
    let items = search["nodes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|n| {
            Some(PullRequest {
                title: n["title"].as_str()?.to_string(),
                url: n["url"].as_str()?.to_string(),
                number: n["number"].as_u64().unwrap_or(0),
                repo: n["repository"]["nameWithOwner"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                draft: n["isDraft"].as_bool().unwrap_or(false),
                updated_at: n["updatedAt"].as_str().unwrap_or_default().to_string(),
            })
        })
        .collect();
    (items, search["issueCount"].as_u64().unwrap_or(0))
}

/// Parse the GraphQL answer. Events are filled in separately.
pub fn parse_graphql(json: &Value, today: NaiveDate, now: u64) -> Option<GithubData> {
    let data = json.get("data")?;
    let viewer = data.get("viewer")?;
    let calendar = &viewer["contributionsCollection"]["contributionCalendar"];
    let days: Vec<ContributionDay> = calendar["weeks"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .flat_map(|week| {
            week["contributionDays"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|d| {
            Some(ContributionDay {
                date: d["date"].as_str()?.to_string(),
                count: d["contributionCount"].as_u64().unwrap_or(0) as u32,
                level: level_number(d["contributionLevel"].as_str().unwrap_or_default()),
            })
        })
        .collect();
    let (today_count, current_streak, longest_streak) = streaks(&days, today);
    let (authored, authored_total) = parse_prs(&data["authored"]);
    let (review_requests, review_total) = parse_prs(&data["reviews"]);
    Some(GithubData {
        login: viewer["login"].as_str()?.to_string(),
        total_year: calendar["totalContributions"].as_u64().unwrap_or(0) as u32,
        days,
        today: today_count,
        current_streak,
        longest_streak,
        authored,
        authored_total,
        review_requests,
        review_total,
        events: Vec::new(),
        fetched_at: now,
    })
}

/// Parse `/users/{login}/events` into the few kinds worth showing.
pub fn parse_events(json: &Value) -> Vec<Event> {
    json.as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|e| {
            let repo = e["repo"]["name"].as_str()?.to_string();
            let repo_url = format!("https://github.com/{repo}");
            let payload = &e["payload"];
            let action = payload["action"].as_str().unwrap_or_default();
            let (kind, summary, url) = match e["type"].as_str()? {
                "PushEvent" => {
                    let count = payload["size"]
                        .as_u64()
                        .or_else(|| payload["commits"].as_array().map(|c| c.len() as u64))
                        .unwrap_or(0);
                    let branch = payload["ref"]
                        .as_str()
                        .unwrap_or_default()
                        .trim_start_matches("refs/heads/");
                    let summary = match count {
                        0 => format!("Pushed to {branch}"),
                        1 => format!("Pushed 1 commit to {branch}"),
                        n => format!("Pushed {n} commits to {branch}"),
                    };
                    let url = payload["head"]
                        .as_str()
                        .map(|sha| format!("{repo_url}/commit/{sha}"))
                        .unwrap_or_else(|| repo_url.clone());
                    ("push", summary, url)
                }
                "PullRequestEvent" => {
                    let pr = &payload["pull_request"];
                    let merged = action == "closed" && pr["merged"].as_bool() == Some(true);
                    let verb = if merged { "Merged" } else { capitalize(action) };
                    let number = payload["number"]
                        .as_u64()
                        .or(pr["number"].as_u64())
                        .unwrap_or(0);
                    let url = pr["html_url"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| repo_url.clone());
                    ("pr", format!("{verb} pull request #{number}"), url)
                }
                "PullRequestReviewEvent" => {
                    let pr = &payload["pull_request"];
                    let number = pr["number"].as_u64().unwrap_or(0);
                    let url = payload["review"]["html_url"]
                        .as_str()
                        .or(pr["html_url"].as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| repo_url.clone());
                    ("review", format!("Reviewed pull request #{number}"), url)
                }
                "IssuesEvent" => {
                    let issue = &payload["issue"];
                    let number = issue["number"].as_u64().unwrap_or(0);
                    let url = issue["html_url"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| repo_url.clone());
                    (
                        "issue",
                        format!("{} issue #{number}", capitalize(action)),
                        url,
                    )
                }
                "IssueCommentEvent" => {
                    let issue = &payload["issue"];
                    let number = issue["number"].as_u64().unwrap_or(0);
                    let url = payload["comment"]["html_url"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| repo_url.clone());
                    ("comment", format!("Commented on #{number}"), url)
                }
                "CreateEvent" => {
                    let kind = payload["ref_type"].as_str().unwrap_or("repository");
                    let summary = match payload["ref"].as_str() {
                        Some(name) => format!("Created {kind} {name}"),
                        None => format!("Created {kind}"),
                    };
                    ("create", summary, repo_url.clone())
                }
                _ => return None,
            };
            Some(Event {
                kind: kind.into(),
                summary,
                repo,
                url,
                created_at: e["created_at"].as_str().unwrap_or_default().to_string(),
            })
        })
        .take(20)
        .collect()
}

fn capitalize(word: &str) -> &'static str {
    match word {
        "opened" => "Opened",
        "closed" => "Closed",
        "reopened" => "Reopened",
        "edited" => "Edited",
        "assigned" => "Assigned",
        "labeled" => "Labelled",
        "synchronize" => "Updated",
        "ready_for_review" => "Marked ready",
        _ => "Updated",
    }
}

// ---------------------------------------------------------------------------
// Token storage
// ---------------------------------------------------------------------------

fn entry() -> Option<keyring::Entry> {
    match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        Ok(entry) => Some(entry),
        Err(e) => {
            log::warn!("github: Credential Manager is unavailable: {e}");
            None
        }
    }
}

fn read_token() -> Option<String> {
    entry()?
        .get_password()
        .ok()
        .filter(|t| !t.trim().is_empty())
}

// ---------------------------------------------------------------------------
// Fetching
// ---------------------------------------------------------------------------

enum Failure {
    InvalidToken,
    RateLimited(Option<String>),
    Offline,
    Other(String),
}

async fn check(response: reqwest::Response) -> Result<reqwest::Response, Failure> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let remaining = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let reset = response
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
        .map(|t| t.with_timezone(&Local).format("%H:%M").to_string());
    match status.as_u16() {
        401 => Err(Failure::InvalidToken),
        403 | 429 if remaining.as_deref() == Some("0") || status.as_u16() == 429 => {
            Err(Failure::RateLimited(reset))
        }
        403 => Err(Failure::Other(
            "GitHub refused the request. The token may lack permission for this data.".into(),
        )),
        code => Err(Failure::Other(format!("GitHub answered {code}"))),
    }
}

fn transport(error: &reqwest::Error) -> Failure {
    if error.is_connect() || error.is_timeout() {
        Failure::Offline
    } else {
        Failure::Other(net::describe(error))
    }
}

async fn fetch(token: &str) -> Result<GithubData, Failure> {
    let client = net::client();
    let response = client
        .post("https://api.github.com/graphql")
        .bearer_auth(token)
        .json(&json!({ "query": QUERY }))
        .send()
        .await
        .map_err(|e| transport(&e))?;
    let json: Value = check(response)
        .await?
        .json()
        .await
        .map_err(|e| transport(&e))?;
    if let Some(errors) = json["errors"].as_array().filter(|e| !e.is_empty()) {
        let message = errors[0]["message"]
            .as_str()
            .unwrap_or("GitHub returned an error");
        if errors
            .iter()
            .any(|e| e["type"].as_str() == Some("RATE_LIMITED"))
        {
            return Err(Failure::RateLimited(None));
        }
        if json.get("data").is_none_or(Value::is_null) {
            return Err(Failure::Other(message.to_string()));
        }
    }
    let today = Local::now().date_naive();
    let mut data = parse_graphql(&json, today, net::now_unix())
        .ok_or_else(|| Failure::Other("GitHub's answer could not be read".into()))?;

    // Recent activity. Failing here is not worth losing the rest for.
    let events = client
        .get(format!(
            "https://api.github.com/users/{}/events?per_page=50",
            data.login
        ))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await;
    match events {
        Ok(response) if response.status().is_success() => {
            if let Ok(json) = response.json::<Value>().await {
                data.events = parse_events(&json);
            }
        }
        Ok(response) => log::warn!("github: events request answered {}", response.status()),
        Err(e) => log::warn!("github: events request failed: {}", net::describe(&e)),
    }
    Ok(data)
}

// ---------------------------------------------------------------------------
// State and refresh loop
// ---------------------------------------------------------------------------

static STATE: Mutex<Option<GithubState>> = Mutex::new(None);
static WAKE: OnceLock<Notify> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn state() -> GithubState {
    STATE.lock().clone().unwrap_or_default()
}

fn publish(next: GithubState) {
    let changed = STATE.lock().replace(next.clone()).as_ref() != Some(&next);
    if changed && let Some(app) = APP.get() {
        let _ = app.emit("github://changed", next);
    }
}

pub fn refresh() {
    if let Some(wake) = WAKE.get() {
        wake.notify_one();
    }
}

async fn refresh_once() {
    let Some(token) = read_token() else {
        publish(GithubState::default());
        return;
    };
    let previous = state()
        .data
        .or_else(|| net::cache_read::<GithubData>(CACHE));
    let next = match fetch(&token).await {
        Ok(data) => {
            net::cache_write(CACHE, &data);
            GithubState {
                status: Status::Ok,
                message: None,
                data: Some(data),
            }
        }
        Err(Failure::InvalidToken) => GithubState {
            status: Status::InvalidToken,
            message: Some(
                "GitHub did not accept the token. It may have expired or been revoked.".into(),
            ),
            data: previous,
        },
        Err(Failure::RateLimited(reset)) => GithubState {
            status: Status::RateLimited,
            message: Some(match reset {
                Some(at) => format!("GitHub's rate limit was reached. It resets at {at}."),
                None => "GitHub's rate limit was reached. Trying again later.".into(),
            }),
            data: previous,
        },
        Err(Failure::Offline) => GithubState {
            status: Status::Offline,
            message: Some("No connection. Showing the last data.".into()),
            data: previous,
        },
        Err(Failure::Other(message)) => {
            log::warn!("github: {message}");
            GithubState {
                status: Status::Error,
                message: Some(message),
                data: previous,
            }
        }
    };
    publish(next);
}

pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let wake = WAKE.get_or_init(Notify::new);
    // Show the cached data straight away while the first fetch runs.
    if read_token().is_some()
        && let Some(cached) = net::cache_read::<GithubData>(CACHE)
    {
        publish(GithubState {
            status: Status::Ok,
            message: None,
            data: Some(cached),
        });
    }
    tauri::async_runtime::spawn(async move {
        loop {
            refresh_once().await;
            tokio::select! {
                () = tokio::time::sleep(REFRESH) => {}
                () = wake.notified() => {}
            }
        }
    });
}

/// Check a token against GitHub and store it if it works. Returns the
/// account name.
pub async fn set_token(token: String) -> Result<String, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("Paste a token first".into());
    }
    let login = match fetch(&token).await {
        Ok(data) => {
            net::cache_write(CACHE, &data);
            let login = data.login.clone();
            publish(GithubState {
                status: Status::Ok,
                message: None,
                data: Some(data),
            });
            login
        }
        Err(Failure::InvalidToken) => return Err("GitHub did not accept this token".into()),
        Err(Failure::RateLimited(_)) => {
            return Err("GitHub's rate limit was reached; try again later".into());
        }
        Err(Failure::Offline) => return Err("No connection to GitHub".into()),
        Err(Failure::Other(message)) => return Err(message),
    };
    entry()
        .ok_or("Windows Credential Manager is unavailable")?
        .set_password(&token)
        .map_err(|e| format!("The token could not be saved: {e}"))?;
    Ok(login)
}

pub fn clear_token() {
    if let Some(entry) = entry() {
        let _ = entry.delete_credential();
    }
    net::cache_remove(CACHE);
    publish(GithubState::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, count: u32) -> ContributionDay {
        ContributionDay {
            date: date.into(),
            count,
            level: u8::from(count > 0),
        }
    }

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn streak_ending_today() {
        let days = [
            day("2026-11-13", 0),
            day("2026-11-14", 2),
            day("2026-11-15", 1),
            day("2026-11-16", 4),
        ];
        assert_eq!(streaks(&days, date("2026-11-16")), (4, 3, 3));
    }

    #[test]
    fn nothing_yet_today_keeps_yesterdays_streak() {
        let days = [
            day("2026-11-14", 2),
            day("2026-11-15", 1),
            day("2026-11-16", 0),
        ];
        assert_eq!(streaks(&days, date("2026-11-16")), (0, 2, 2));
    }

    #[test]
    fn a_missed_day_before_yesterday_ends_the_streak() {
        let days = [
            day("2026-11-13", 5),
            day("2026-11-14", 0),
            day("2026-11-15", 0),
            day("2026-11-16", 0),
        ];
        assert_eq!(streaks(&days, date("2026-11-16")), (0, 0, 1));
    }

    #[test]
    fn longest_streak_is_the_longest_run_anywhere() {
        let days = [
            day("2026-01-01", 1),
            day("2026-01-02", 1),
            day("2026-01-03", 1),
            day("2026-01-04", 0),
            day("2026-01-05", 1),
        ];
        assert_eq!(streaks(&days, date("2026-01-05")), (1, 1, 3));
    }

    #[test]
    fn days_after_today_are_ignored() {
        // The calendar can include tomorrow when GitHub's time zone is ahead.
        let days = [
            day("2026-11-15", 1),
            day("2026-11-16", 1),
            day("2026-11-17", 0),
        ];
        assert_eq!(streaks(&days, date("2026-11-16")), (1, 2, 2));
    }

    #[test]
    fn a_gap_in_the_dates_breaks_a_run() {
        let days = [day("2026-11-10", 1), day("2026-11-12", 1)];
        assert_eq!(streaks(&days, date("2026-11-12")), (1, 1, 1));
    }

    #[test]
    fn empty_calendar() {
        assert_eq!(streaks(&[], date("2026-11-16")), (0, 0, 0));
    }

    #[test]
    fn parses_the_graphql_answer() {
        let answer = json!({ "data": {
            "viewer": { "login": "octo", "contributionsCollection": { "contributionCalendar": {
                "totalContributions": 812,
                "weeks": [
                    { "contributionDays": [
                        { "date": "2026-11-15", "contributionCount": 3, "contributionLevel": "SECOND_QUARTILE" },
                        { "date": "2026-11-16", "contributionCount": 0, "contributionLevel": "NONE" }
                    ]}
                ]
            }}},
            "authored": { "issueCount": 12, "nodes": [
                { "title": "Fix it", "url": "https://github.com/o/r/pull/7", "number": 7, "isDraft": true,
                  "updatedAt": "2026-11-16T10:00:00Z", "repository": { "nameWithOwner": "o/r" } },
                {}
            ]},
            "reviews": { "issueCount": 0, "nodes": [] },
            "rateLimit": { "remaining": 4990, "resetAt": "2026-11-16T11:00:00Z" }
        }});
        let data = parse_graphql(&answer, date("2026-11-16"), 99).unwrap();
        assert_eq!(data.login, "octo");
        assert_eq!(data.total_year, 812);
        assert_eq!(data.days.len(), 2);
        assert_eq!(data.days[0].level, 2);
        assert_eq!((data.today, data.current_streak), (0, 1));
        assert_eq!(data.authored.len(), 1, "malformed nodes are skipped");
        assert_eq!(
            (data.authored_total, data.authored[0].repo.as_str()),
            (12, "o/r")
        );
        assert!(data.authored[0].draft);
        assert!(data.review_requests.is_empty());
    }

    #[test]
    fn an_answer_without_data_is_rejected() {
        assert!(
            parse_graphql(
                &json!({ "errors": [{ "message": "Bad credentials" }] }),
                date("2026-11-16"),
                0
            )
            .is_none()
        );
    }

    #[test]
    fn parses_events_and_skips_uninteresting_ones() {
        let events = json!([
            { "type": "PushEvent", "repo": { "name": "o/r" }, "created_at": "2026-11-16T09:00:00Z",
              "payload": { "size": 3, "ref": "refs/heads/main", "head": "abc123" } },
            { "type": "PullRequestEvent", "repo": { "name": "o/r" }, "created_at": "2026-11-16T08:00:00Z",
              "payload": { "action": "closed", "number": 7,
                           "pull_request": { "merged": true, "html_url": "https://github.com/o/r/pull/7" } } },
            { "type": "WatchEvent", "repo": { "name": "x/y" }, "payload": {} },
            { "type": "IssuesEvent", "repo": { "name": "o/r" },
              "payload": { "action": "opened", "issue": { "number": 3, "html_url": "https://github.com/o/r/issues/3" } } },
            { "type": "PushEvent", "repo": { "name": "o/r" }, "payload": { "ref": "refs/heads/dev" } }
        ]);
        let parsed = parse_events(&events);
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].summary, "Pushed 3 commits to main");
        assert_eq!(parsed[0].url, "https://github.com/o/r/commit/abc123");
        assert_eq!(parsed[1].summary, "Merged pull request #7");
        assert_eq!(parsed[1].url, "https://github.com/o/r/pull/7");
        assert_eq!(parsed[2].summary, "Opened issue #3");
        // A push event without size or head (newer, slimmer payloads).
        assert_eq!(parsed[3].summary, "Pushed to dev");
        assert_eq!(parsed[3].url, "https://github.com/o/r");
    }
}
