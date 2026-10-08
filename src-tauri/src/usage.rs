//! Usage limits per tool, read from what each tool already saves on this Mac.
//! The numbers are as old as the tool's own last check, unless `refresh_claude`
//! has Claude Code check again first.

use crate::model::{ToolKind, ToolUsage, UsageWindow};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const CODEX_FILES_TO_TRY: usize = 20;
const CODEX_TAIL_BYTES: u64 = 4 * 1024 * 1024;
const MINUTES_PER_HOUR: u64 = 60;
const MINUTES_PER_DAY: u64 = 24 * MINUTES_PER_HOUR;

/// One entry per installed tool, in the order given.
#[must_use]
pub fn tool_usage(home: &Path, installed: &[ToolKind]) -> Vec<ToolUsage> {
    let now = Utc::now();
    installed
        .iter()
        .map(|&tool| match tool {
            ToolKind::ClaudeCode => claude_usage(home, now),
            ToolKind::Codex => codex_usage(home, now),
            ToolKind::Antigravity => unavailable(
                tool,
                "Antigravity keeps its quota on Google's servers, not on this Mac",
            ),
            _ => unavailable(tool, "Usage is not read for this tool yet"),
        })
        .collect()
}

fn unavailable(tool: ToolKind, reason: &str) -> ToolUsage {
    ToolUsage {
        tool,
        checked_at: None,
        windows: Vec::new(),
        unavailable: Some(reason.to_owned()),
    }
}

fn iso(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// A window whose reset time has passed has reset since the check: it reads 0.
fn window(
    id: String,
    label: String,
    percent: f64,
    resets_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> UsageWindow {
    match resets_at {
        Some(at) if at > now => UsageWindow {
            id,
            label,
            percent,
            resets_at: Some(iso(at)),
        },
        Some(_) => UsageWindow {
            id,
            label,
            percent: 0.0,
            resets_at: None,
        },
        None => UsageWindow {
            id,
            label,
            percent,
            resets_at: None,
        },
    }
}

// Claude Code: ~/.claude.json -> cachedUsageUtilization

const CLAUDE_UNCHECKED: &str =
    "Claude Code has not checked its limits yet. Run /usage in Claude Code";

#[derive(Deserialize)]
struct ClaudeFile {
    #[serde(rename = "cachedUsageUtilization")]
    cached: Option<ClaudeCache>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCache {
    fetched_at_ms: i64,
    utilization: ClaudeUtilization,
}

#[derive(Deserialize)]
struct ClaudeUtilization {
    limits: Vec<ClaudeLimit>,
}

#[derive(Deserialize)]
struct ClaudeLimit {
    kind: String,
    percent: f64,
    resets_at: Option<String>,
    scope: Option<ClaudeScope>,
    is_active: bool,
}

#[derive(Deserialize)]
struct ClaudeScope {
    model: ClaudeModel,
}

#[derive(Deserialize)]
struct ClaudeModel {
    display_name: String,
}

fn slug(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn claude_window(limit: &ClaudeLimit, now: DateTime<Utc>) -> Option<UsageWindow> {
    let (id, label) = match (limit.kind.as_str(), &limit.scope) {
        ("session", _) => ("session".to_owned(), "5-hour session".to_owned()),
        ("weekly_all", _) => ("weekly_all".to_owned(), "Week".to_owned()),
        ("weekly_scoped", Some(scope)) if limit.percent > 0.0 || limit.is_active => {
            let name = &scope.model.display_name;
            (
                format!("weekly_scoped-{}", slug(name)),
                format!("Week · {name}"),
            )
        }
        _ => return None,
    };
    let resets_at = limit
        .resets_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc));
    Some(window(id, label, limit.percent, resets_at, now))
}

fn parse_claude(json: &str, now: DateTime<Utc>) -> ToolUsage {
    let Some(cache) = serde_json::from_str::<ClaudeFile>(json)
        .ok()
        .and_then(|f| f.cached)
    else {
        return unavailable(ToolKind::ClaudeCode, CLAUDE_UNCHECKED);
    };
    ToolUsage {
        tool: ToolKind::ClaudeCode,
        checked_at: DateTime::from_timestamp_millis(cache.fetched_at_ms).map(iso),
        windows: cache
            .utilization
            .limits
            .iter()
            .filter_map(|l| claude_window(l, now))
            .collect(),
        unavailable: None,
    }
}

/// Has Claude Code fetch its usage again, which it saves to `~/.claude.json`.
/// No transcript, settings hooks or MCP servers. Within a minute of its last
/// fetch Claude Code skips it and keeps the saved numbers.
pub fn refresh_claude() -> Result<(), String> {
    crate::options::run_capped(
        "claude -p /usage --no-session-persistence --setting-sources '' --strict-mcp-config",
    )
    .map(drop)
    .ok_or_else(|| "Claude Code did not check its usage".to_owned())
}

fn claude_usage(home: &Path, now: DateTime<Utc>) -> ToolUsage {
    fs::read_to_string(home.join(".claude.json")).map_or_else(
        |_| unavailable(ToolKind::ClaudeCode, CLAUDE_UNCHECKED),
        |json| parse_claude(&json, now),
    )
}

// Codex: ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl, token_count events

const CODEX_UNREPORTED: &str = "Codex has not reported its limits yet";

#[derive(Deserialize)]
struct CodexLine {
    timestamp: String,
    payload: CodexPayload,
}

#[derive(Deserialize)]
struct CodexPayload {
    rate_limits: Option<CodexLimits>,
}

#[derive(Deserialize)]
struct CodexLimits {
    limit_id: String,
    primary: Option<CodexWindow>,
    secondary: Option<CodexWindow>,
}

#[derive(Deserialize)]
struct CodexWindow {
    used_percent: f64,
    window_minutes: u64,
    resets_at: i64,
}

fn codex_label(minutes: u64) -> String {
    match minutes {
        300 => "5-hour session".to_owned(),
        10080 => "Week".to_owned(),
        m if m % MINUTES_PER_DAY == 0 => format!("{}-day", m / MINUTES_PER_DAY),
        m => format!("{}-hour", m / MINUTES_PER_HOUR),
    }
}

/// The newest complete line (scanning backwards) with `codex` limits that have a window.
fn parse_codex_tail(text: &str, now: DateTime<Utc>) -> Option<ToolUsage> {
    let (checked_at, limits) = text.lines().rev().find_map(|line| {
        if !line.contains("\"rate_limits\":{") {
            return None;
        }
        let parsed: CodexLine = serde_json::from_str(line).ok()?;
        let limits = parsed.payload.rate_limits?;
        let has_windows = limits.primary.is_some() || limits.secondary.is_some();
        (limits.limit_id == "codex" && has_windows).then_some((parsed.timestamp, limits))
    })?;
    let windows = [("primary", limits.primary), ("secondary", limits.secondary)]
        .into_iter()
        .filter_map(|(id, w)| {
            let w = w?;
            let resets_at = DateTime::from_timestamp(w.resets_at, 0);
            Some(window(
                id.to_owned(),
                codex_label(w.window_minutes),
                w.used_percent,
                resets_at,
                now,
            ))
        })
        .collect();
    Some(ToolUsage {
        tool: ToolKind::Codex,
        checked_at: Some(checked_at),
        windows,
        unavailable: None,
    })
}

/// Files can be 100+ MB, so only the last `CODEX_TAIL_BYTES` are read.
fn read_tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(CODEX_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    // A tail that starts mid-line drops that partial first line.
    Some(if start > 0 {
        text.split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or_default()
            .to_owned()
    } else {
        text.into_owned()
    })
}

fn newest_rollouts(sessions: &Path) -> Vec<PathBuf> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut dirs = vec![sessions.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "jsonl")
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("rollout-"))
                && let Ok(modified) = entry.metadata().and_then(|m| m.modified())
            {
                files.push((modified, path));
            }
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files
        .into_iter()
        .take(CODEX_FILES_TO_TRY)
        .map(|(_, p)| p)
        .collect()
}

fn codex_usage(home: &Path, now: DateTime<Utc>) -> ToolUsage {
    newest_rollouts(&home.join(".codex").join("sessions"))
        .iter()
        .find_map(|path| parse_codex_tail(&read_tail(path)?, now))
        .unwrap_or_else(|| unavailable(ToolKind::Codex, CODEX_UNREPORTED))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    const CLAUDE: &str = r#"{"other":{"x":1},"cachedUsageUtilization":{"fetchedAtMs":1791306910916,"accountUuid":"a","utilization":{"extra":[1],"limits":[
      {"kind":"session","group":"session","percent":8,"severity":"normal","resets_at":"2026-10-06T18:59:59.834827+00:00","scope":null,"is_active":false},
      {"kind":"weekly_all","group":"weekly","percent":40,"severity":"normal","resets_at":"2026-10-10T18:59:59+00:00","scope":null,"is_active":true},
      {"kind":"weekly_scoped","group":"weekly","percent":0,"severity":"normal","resets_at":"2026-10-10T19:00:00+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null},"is_active":false},
      {"kind":"weekly_scoped","group":"weekly","percent":12,"severity":"normal","resets_at":"2026-10-10T19:00:00+00:00","scope":{"model":{"id":null,"display_name":"Opus 5"},"surface":null},"is_active":false},
      {"kind":"mystery","group":"x","percent":50,"severity":"normal","resets_at":null,"scope":null,"is_active":true}]}}}"#;

    #[test]
    fn claude_windows_follow_the_rules() {
        let usage = parse_claude(CLAUDE, at("2026-10-08T00:00:00Z"));
        assert_eq!(
            usage.checked_at.as_deref(),
            Some("2026-10-06T17:15:10.916Z")
        );
        let got: Vec<_> = usage
            .windows
            .iter()
            .map(|w| (w.id.as_str(), w.label.as_str(), w.percent))
            .collect();
        assert_eq!(
            got,
            [
                ("session", "5-hour session", 0.0),
                ("weekly_all", "Week", 40.0),
                ("weekly_scoped-opus-5", "Week · Opus 5", 12.0),
            ]
        );
        assert_eq!(usage.windows[0].resets_at, None);
        assert_eq!(
            usage.windows[1].resets_at.as_deref(),
            Some("2026-10-10T18:59:59.000Z")
        );
    }

    #[test]
    fn claude_missing_key_is_unavailable() {
        let usage = parse_claude("{}", at("2026-10-08T00:00:00Z"));
        assert_eq!(usage.unavailable.as_deref(), Some(CLAUDE_UNCHECKED));
        assert!(usage.windows.is_empty());
    }

    #[test]
    fn codex_takes_the_newest_non_null_event() {
        let text = concat!(
            r#"{"timestamp":"2026-10-06T10:00:00.000Z","payload":{"rate_limits":{"limit_id":"codex","primary":{"used_percent":1.0,"window_minutes":300,"resets_at":1791318694}}}}"#,
            "\n",
            r#"{"timestamp":"2026-10-06T16:11:06.856Z","payload":{"rate_limits":{"limit_id":"codex","primary":{"used_percent":5.0,"window_minutes":300,"resets_at":1791318694},"secondary":{"used_percent":9.0,"window_minutes":10080,"resets_at":1791812569}}}}"#,
            "\n",
            r#"{"timestamp":"2026-10-06T16:12:00.000Z","payload":{"rate_limits":null}}"#,
            "\n",
            r#"{"timestamp":"2026-10-06T16:13:00.000Z","payload":{"type":"other"}}"#,
            "\n",
        );
        let usage = parse_codex_tail(text, at("2026-10-06T17:00:00Z")).unwrap();
        assert_eq!(
            usage.checked_at.as_deref(),
            Some("2026-10-06T16:11:06.856Z")
        );
        let got: Vec<_> = usage
            .windows
            .iter()
            .map(|w| (w.label.as_str(), w.percent))
            .collect();
        assert_eq!(got, [("5-hour session", 5.0), ("Week", 9.0)]);
        assert!(usage.windows.iter().all(|w| w.resets_at.is_some()));
    }

    #[test]
    fn codex_skips_trailing_premium_and_empty_events() {
        let text = concat!(
            r#"{"timestamp":"2026-10-06T16:11:06.856Z","payload":{"rate_limits":{"limit_id":"codex","primary":{"used_percent":5.0,"window_minutes":300,"resets_at":1791318694},"secondary":null}}}"#,
            "\n",
            r#"{"timestamp":"2026-10-06T16:12:00.000Z","payload":{"rate_limits":{"limit_id":"premium","primary":null,"secondary":null}}}"#,
            "\n",
            r#"{"timestamp":"2026-10-06T16:13:00.000Z","payload":{"rate_limits":{"limit_id":"codex","primary":null,"secondary":null}}}"#,
            "\n",
        );
        let usage = parse_codex_tail(text, at("2026-10-06T17:00:00Z")).unwrap();
        assert_eq!(
            usage.checked_at.as_deref(),
            Some("2026-10-06T16:11:06.856Z")
        );
        assert_eq!(usage.windows.len(), 1);
    }

    #[test]
    fn codex_passed_reset_reads_zero_and_odd_windows_get_labels() {
        let text = r#"{"timestamp":"t","payload":{"rate_limits":{"limit_id":"codex","primary":{"used_percent":5.0,"window_minutes":300,"resets_at":1000},"secondary":{"used_percent":3.0,"window_minutes":2880,"resets_at":4000000000}}}}"#;
        let usage = parse_codex_tail(text, at("2026-10-06T17:00:00Z")).unwrap();
        assert_eq!(
            (usage.windows[0].percent, usage.windows[0].resets_at.clone()),
            (0.0, None)
        );
        assert_eq!(usage.windows[1].label, "2-day");
        assert_eq!(codex_label(180), "3-hour");
    }

    #[test]
    fn codex_without_limits_is_none() {
        assert!(
            parse_codex_tail(
                r#"{"timestamp":"t","payload":{"rate_limits":null}}"#,
                Utc::now()
            )
            .is_none()
        );
    }

    #[test]
    fn other_tools_are_unavailable_in_order() {
        let usage = tool_usage(
            Path::new("/nonexistent"),
            &[ToolKind::Cursor, ToolKind::Antigravity],
        );
        assert_eq!(usage[0].tool, ToolKind::Cursor);
        assert!(usage[1].unavailable.as_deref().unwrap().contains("Google"));
    }

    #[test]
    #[ignore = "reads the real home folder"]
    fn print_real_usage() {
        let home = dirs::home_dir().unwrap();
        let usage = tool_usage(&home, crate::tools::LISTED);
        println!("{}", serde_json::to_string_pretty(&usage).unwrap());
    }
}
