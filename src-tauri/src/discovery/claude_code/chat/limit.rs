//! What stands at the end of a conversation's main chain: Claude Code's own
//! record that the whole subscription is used up, another API error, or
//! nothing (handover spec 5.6). The record's shape was seen when a local
//! listener replayed a used-up allowance to Claude Code 2.1.323. The transcript
//! format is undocumented (<https://code.claude.com/docs/en/sessions>), so every
//! field is read leniently and anything unexpected proves nothing.

use super::map::lenient;
use crate::handover::Window;
use crate::handover::record::RESET_HORIZON;
use serde::Deserialize;

/// What stands at the end of a conversation's main chain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Reading {
    /// No API error, or the model has answered since.
    #[default]
    Clear,
    /// Claude Code's own record that the whole subscription is used up.
    UsedUp {
        window: Window,
        resets_at: Option<i64>,
    },
    /// An API error that proves nothing about the subscription.
    Other(Why),
}

/// What an API error that is not the proof was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Throttle,
    /// A limit on one model family (Opus or Sonnet): other models may still answer.
    ModelLimit,
    Authentication,
    ServerFault,
    Unreadable,
    Unknown,
}

impl From<&Reading> for crate::handover::Allowance {
    fn from(reading: &Reading) -> Self {
        match reading {
            Reading::Clear => Self::Clear,
            Reading::UsedUp { window, resets_at } => Self::UsedUp {
                window: Some(*window),
                reset_at: *resets_at,
                at: None,
            },
            Reading::Other(_) => Self::Unproven,
        }
    }
}

/// `apiErrorParams` on an API error line. Its key is snake case inside a camel case line.
#[derive(Deserialize, Default)]
pub(super) struct Params {
    #[serde(default, deserialize_with = "lenient")]
    rate_limit_info: Option<Info>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Info {
    #[serde(default, deserialize_with = "lenient")]
    status: Option<String>,
    /// Unix seconds.
    #[serde(default, deserialize_with = "lenient")]
    resets_at: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    rate_limit_type: Option<String>,
}

/// The parts of one main-chain transcript line the reading looks at.
pub(super) struct Said<'a> {
    pub(super) kind: &'a str,
    pub(super) api_error_message: bool,
    pub(super) model: Option<&'a str>,
    pub(super) error: Option<&'a str>,
    pub(super) api_error: Option<&'a str>,
    pub(super) params: Option<&'a Params>,
}

/// The reading, kept up to date line by line.
#[derive(Debug, Default)]
pub(super) struct Scan {
    reading: Reading,
}

impl Scan {
    pub(super) fn reading(&self) -> &Reading {
        &self.reading
    }

    /// A line that could not be read at all.
    pub(super) fn unreadable(&mut self) {
        self.reading = Reading::Other(Why::Unreadable);
    }

    /// One main-chain line. A sidechain or meta line never comes here.
    pub(super) fn see(&mut self, line: &Said) {
        if line.kind != "assistant" {
            return;
        }
        if !line.api_error_message {
            // A model's own reply; `<synthetic>` and the like are Claude Code's.
            if line.model.is_some_and(|m| !m.starts_with('<')) {
                self.reading = Reading::Clear;
            }
            return;
        }
        self.reading = if line.api_error == Some("usage_limit_reached") {
            used_up(line.params)
        } else {
            Reading::Other(match line.error {
                Some("rate_limit") => Why::Throttle,
                Some(
                    "authentication_failed" | "oauth_org_not_allowed" | "cloud_credential_error",
                ) => Why::Authentication,
                Some("server_error" | "overloaded") => Why::ServerFault,
                _ => Why::Unknown,
            })
        };
    }
}

/// What a `usage_limit_reached` error says. Only a rejected whole-subscription
/// window is the proof (spec G2).
fn used_up(params: Option<&Params>) -> Reading {
    let Some(info) = params
        .and_then(|p| p.rate_limit_info.as_ref())
        .filter(|i| i.status.as_deref() == Some("rejected"))
    else {
        return Reading::Other(Why::Unreadable);
    };
    let window = match info.rate_limit_type.as_deref() {
        Some("five_hour") => Window::FiveHour,
        Some("seven_day") => Window::SevenDay,
        Some("seven_day_opus" | "seven_day_sonnet") => return Reading::Other(Why::ModelLimit),
        _ => return Reading::Other(Why::Unknown),
    };
    Reading::UsedUp {
        window,
        resets_at: info.resets_at.filter(|r| *r <= latest_reset()),
    }
}

/// The furthest reset taken as real: `RESET_HORIZON` on. A larger number (milliseconds, say)
/// would be a record that never lapses, so it counts as no reset (the unknown hold).
fn latest_reset() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    now.saturating_add(RESET_HORIZON)
}

#[cfg(test)]
mod tests {
    use super::super::ChatReader;
    use super::super::map::Mapper;
    use super::*;
    use std::fs;
    use std::io::Write;

    // The entry Claude Code 2.1.323 wrote when a local listener replayed a used-up
    // allowance (or a throttle) to it on 2026-11-13. A replay, not Anthropic's own
    // reply, and the transcript format is undocumented.
    const LIMIT: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T10:53:07.124Z","message":{"id":"m1","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"You've hit your 5-hour limit · resets 12:53pm (UTC)"}]},"apiError":"usage_limit_reached","apiErrorParams":{"rate_limit_info":{"status":"rejected","resetsAt":1794574385,"rateLimitType":"five_hour","isUsingOverage":false}},"quotaLimits":{"status":"rejected","resetsAt":1794574385,"unifiedRateLimitFallbackAvailable":false,"rateLimitType":"five_hour","isUsingOverage":false},"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429}"#;
    const THROTTLE: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:17:26.534Z","message":{"id":"m2","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"API Error: Server is temporarily limiting requests (not your usage limit)"}]},"apiErrorIsTransient":true,"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429}"#;

    // Composed from the throttle line and the key names of API error entries seen
    // in transcripts on one machine, not from a replay.
    const AUTH: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:20:00.000Z","message":{"id":"m3","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"API Error: 401"}]},"error":"authentication_failed","isApiErrorMessage":true,"apiErrorStatus":401}"#;
    const SERVER: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:21:00.000Z","message":{"id":"m4","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"API Error: 500"}]},"error":"server_error","isApiErrorMessage":true,"apiErrorStatus":500}"#;

    const REPLY: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"m5","role":"assistant","model":"claude-opus-4-5","content":[{"type":"text","text":"Done."}]}}"#;
    const SYNTHETIC: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"m6","role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"No response requested."}]}}"#;
    const USER: &str = r#"{"type":"user","message":{"role":"user","content":"carry on"}}"#;
    const SYSTEM: &str =
        r#"{"type":"system","subtype":"informational","content":"Waiting for the limit to reset"}"#;

    const USED_UP: Reading = Reading::UsedUp {
        window: Window::FiveHour,
        resets_at: Some(1_794_574_385),
    };

    /// The limit line with `from` replaced by `to` (each must be there once).
    fn limit_with(from: &str, to: &str) -> String {
        assert_eq!(LIMIT.matches(from).count(), 1, "{from}");
        LIMIT.replace(from, to)
    }

    fn reading(lines: &[&str]) -> Reading {
        let mut mapper = Mapper::default();
        let mut at = 0;
        for line in lines {
            mapper.line(at, line);
            at += 1000;
        }
        mapper.limit().clone()
    }

    #[test]
    fn the_proof_table() {
        let seven_day = limit_with(
            r#""rateLimitType":"five_hour","isUsingOverage":false}},"#,
            r#""rateLimitType":"seven_day","isUsingOverage":false}},"#,
        );
        let sidechain = limit_with(r#""isSidechain":false"#, r#""isSidechain":true"#);
        let no_reset = limit_with(
            r#""resetsAt":1794574385,"rateLimitType""#,
            r#""resetsAt":"soon","rateLimitType""#,
        );
        let window = |w: &str| {
            limit_with(
                r#""rateLimitType":"five_hour","isUsingOverage":false}},"#,
                &format!(r#""rateLimitType":{w},"isUsingOverage":false}}}},"#),
            )
        };
        let opus = window(r#""seven_day_opus""#);
        let sonnet = window(r#""seven_day_sonnet""#);
        let overage = window(r#""overage""#);
        let made_up = window(r#""fortnight""#);
        let no_window = limit_with(
            r#""rateLimitType":"five_hour","isUsingOverage":false}},"#,
            r#""isUsingOverage":false}},"#,
        );
        let not_flagged = limit_with(
            r#""isApiErrorMessage":true"#,
            r#""isApiErrorMessage":false"#,
        );
        let rows: Vec<(&str, Vec<&str>, Reading)> = vec![
            ("nothing", vec![], Reading::Clear),
            ("the limit", vec![LIMIT], USED_UP),
            (
                "seven days",
                vec![&seven_day],
                Reading::UsedUp {
                    window: Window::SevenDay,
                    resets_at: Some(1_794_574_385),
                },
            ),
            ("the limit on a sidechain", vec![&sidechain], Reading::Clear),
            (
                "the limit not flagged as an error",
                vec![&not_flagged],
                Reading::Clear,
            ),
            (
                "a reset that is not a number",
                vec![&no_reset],
                Reading::UsedUp {
                    window: Window::FiveHour,
                    resets_at: None,
                },
            ),
            ("opus", vec![&opus], Reading::Other(Why::ModelLimit)),
            ("sonnet", vec![&sonnet], Reading::Other(Why::ModelLimit)),
            ("overage", vec![&overage], Reading::Other(Why::Unknown)),
            (
                "a made-up window",
                vec![&made_up],
                Reading::Other(Why::Unknown),
            ),
            ("no window", vec![&no_window], Reading::Other(Why::Unknown)),
        ];
        for (name, lines, want) in rows {
            assert_eq!(reading(&lines), want, "{name}");
        }
    }

    #[test]
    fn other_api_errors_prove_nothing() {
        let billing = THROTTLE.replace(r#""error":"rate_limit""#, r#""error":"billing_error""#);
        let overloaded = SERVER.replace("server_error", "overloaded");
        let oauth = AUTH.replace("authentication_failed", "oauth_org_not_allowed");
        let cloud = AUTH.replace("authentication_failed", "cloud_credential_error");
        let no_error = THROTTLE.replace(r#""error":"rate_limit","#, "");
        let rows: Vec<(&str, Vec<&str>, Reading)> = vec![
            (
                "the throttle",
                vec![THROTTLE],
                Reading::Other(Why::Throttle),
            ),
            (
                "authentication",
                vec![AUTH],
                Reading::Other(Why::Authentication),
            ),
            ("oauth", vec![&oauth], Reading::Other(Why::Authentication)),
            (
                "cloud credential",
                vec![&cloud],
                Reading::Other(Why::Authentication),
            ),
            (
                "server fault",
                vec![SERVER],
                Reading::Other(Why::ServerFault),
            ),
            (
                "overloaded",
                vec![&overloaded],
                Reading::Other(Why::ServerFault),
            ),
            ("billing", vec![&billing], Reading::Other(Why::Unknown)),
            (
                "no error word",
                vec![&no_error],
                Reading::Other(Why::Unknown),
            ),
        ];
        for (name, lines, want) in rows {
            assert_eq!(reading(&lines), want, "{name}");
        }
    }

    #[test]
    fn only_a_models_own_reply_clears_the_proof() {
        let side_reply = REPLY.replace(r#""isSidechain":false"#, r#""isSidechain":true"#);
        let rows: Vec<(&str, Vec<&str>, Reading)> = vec![
            (
                "a real reply after the limit",
                vec![LIMIT, REPLY],
                Reading::Clear,
            ),
            (
                "a synthetic line after the limit",
                vec![LIMIT, SYNTHETIC],
                USED_UP,
            ),
            ("a user line after the limit", vec![LIMIT, USER], USED_UP),
            (
                "a system line after the limit",
                vec![LIMIT, SYSTEM],
                USED_UP,
            ),
            ("a blank line after the limit", vec![LIMIT, "  "], USED_UP),
            (
                "a sidechain reply after the limit",
                vec![LIMIT, &side_reply],
                USED_UP,
            ),
            (
                "a throttle after the limit",
                vec![LIMIT, THROTTLE],
                Reading::Other(Why::Throttle),
            ),
            ("the limit after a real reply", vec![REPLY, LIMIT], USED_UP),
            ("the limit after a throttle", vec![THROTTLE, LIMIT], USED_UP),
        ];
        for (name, lines, want) in rows {
            assert_eq!(reading(&lines), want, "{name}");
        }
    }

    #[test]
    fn an_unreadable_limit_or_line_proves_nothing() {
        let no_params = limit_with(
            r#""apiErrorParams":{"rate_limit_info":{"status":"rejected","resetsAt":1794574385,"rateLimitType":"five_hour","isUsingOverage":false}},"#,
            "",
        );
        let info_string = limit_with(
            r#"{"rate_limit_info":{"status":"rejected","resetsAt":1794574385,"rateLimitType":"five_hour","isUsingOverage":false}}"#,
            r#"{"rate_limit_info":"rejected"}"#,
        );
        let warning = limit_with(
            r#"{"rate_limit_info":{"status":"rejected""#,
            r#"{"rate_limit_info":{"status":"allowed_warning""#,
        );
        let no_status = limit_with(
            r#"{"rate_limit_info":{"status":"rejected","#,
            r#"{"rate_limit_info":{"#,
        );
        let long = format!(
            "{{\"type\":\"assistant\",\"pad\":\"{}\"}}",
            "x".repeat(super::super::map::LINE_MAX)
        );
        let unreadable: [(&str, &str); 7] = [
            ("no apiErrorParams", &no_params),
            ("rate_limit_info a string", &info_string),
            ("status allowed_warning", &warning),
            ("no status", &no_status),
            ("not json", "not json"),
            ("an array", "[1]"),
            ("too long", &long),
        ];
        for (name, line) in unreadable {
            let want = Reading::Other(Why::Unreadable);
            assert_eq!(reading(&[line]), want, "{name} after nothing");
            assert_eq!(reading(&[LIMIT, line]), want, "{name} after the limit");
        }
    }

    #[test]
    fn a_reading_gives_the_neutral_allowance() {
        use crate::handover::Allowance;
        let rows = [
            (Reading::Clear, Allowance::Clear),
            (
                USED_UP,
                Allowance::UsedUp {
                    window: Some(Window::FiveHour),
                    reset_at: Some(1_794_574_385),
                    at: None,
                },
            ),
            (Reading::Other(Why::Throttle), Allowance::Unproven),
        ];
        for (reading, want) in rows {
            assert_eq!(Allowance::from(&reading), want, "{reading:?}");
        }
    }

    #[test]
    fn the_chat_view_shows_the_limit_as_before() {
        let mut mapper = Mapper::default();
        mapper.line(0, LIMIT);
        let kept = mapper.kept();
        assert_eq!(kept.len(), 1);
        assert!(matches!(&kept[0], crate::structured::Entry::Notice { .. }));
    }

    #[test]
    fn the_reader_follows_the_proof_in_a_transcript() {
        let home = std::env::temp_dir().join(format!("ob-limit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let dir = home.join(".claude/projects/p");
        fs::create_dir_all(&dir).expect("dirs");
        let path = dir.join("s1.jsonl");
        let append = |line: &str| {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("open");
            writeln!(file, "{line}").expect("write");
        };
        let mut reader = ChatReader::new(&home, "s1");
        assert_eq!(reader.limit(), &Reading::Clear);
        append(USER);
        append(LIMIT);
        reader.read();
        assert_eq!(reader.limit(), &USED_UP);
        append(REPLY);
        reader.read();
        assert_eq!(reader.limit(), &Reading::Clear);
        let _ = fs::remove_dir_all(&home);
    }
}
