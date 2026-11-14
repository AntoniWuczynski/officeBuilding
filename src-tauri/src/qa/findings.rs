//! The reviewer's report: its final reply, and the Stop hook input that carries it.

use serde::Deserialize;

pub use super::record::Verdict;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    High,
    Medium,
    Low,
}

/// One thing the reviewer found.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Finding {
    pub file: String,
    #[serde(default)]
    pub line: Option<u32>,
    pub severity: Severity,
    pub summary: String,
    #[serde(default)]
    pub failure_scenario: String,
}

/// Everything the reviewer reported.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Report {
    pub verdict: Verdict,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

/// The most findings one review carries. The rest are dropped.
pub const MAX_FINDINGS: usize = 100;

const NO_REPLY: &str = "the reviewer finished without a reply";
const UNREADABLE: &str = "the reviewer's report could not be read";
const NOT_FINDINGS: &str = "the reviewer's reply was not the findings it was asked for";

/// The parts of a Stop hook's input this module reads. The shape is Claude Code's.
#[derive(Deserialize)]
struct Stop {
    #[serde(default)]
    last_assistant_message: Option<String>,
}

/// The report in a Stop hook's input (the JSON Claude Code hands the hook).
///
/// # Errors
/// When the input is not JSON, holds no reply, or the reply is not the findings.
pub fn from_stop(hook_input: &str) -> Result<Report, String> {
    let stop: Stop = serde_json::from_str(hook_input).map_err(|_| UNREADABLE.to_owned())?;
    match stop.last_assistant_message {
        Some(message) if !message.trim().is_empty() => from_reply(&message),
        _ => Err(NO_REPLY.to_owned()),
    }
}

/// The report in the reviewer's final reply.
///
/// # Errors
/// When the reply holds no JSON of the findings' shape.
pub fn from_reply(reply: &str) -> Result<Report, String> {
    let json = last_fenced_json(reply).or_else(|| braced(reply));
    let mut report: Report = json
        .and_then(|text| serde_json::from_str(text).ok())
        .ok_or_else(|| NOT_FINDINGS.to_owned())?;
    tidy(&mut report);
    Ok(report)
}

/// The inside of the last block opened by a "```json" line and closed by a "```" line.
fn last_fenced_json(reply: &str) -> Option<&str> {
    let mut found = None;
    let mut open: Option<usize> = None;
    let mut offset = 0;
    for line in reply.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        match open {
            None if line.trim_start().starts_with("```json") => open = Some(offset),
            Some(from) if line.trim() == "```" => {
                found = reply.get(from..start);
                open = None;
            }
            _ => {}
        }
    }
    found
}

/// The text from the first `{` to the last `}`.
fn braced(reply: &str) -> Option<&str> {
    reply.get(reply.find('{')?..=reply.rfind('}')?)
}

fn tidy(report: &mut Report) {
    report.findings.retain_mut(|finding| {
        let file = finding.file.trim();
        finding.file = file.strip_prefix("./").unwrap_or(file).to_owned();
        finding.summary = finding.summary.trim().to_owned();
        if finding.line == Some(0) {
            finding.line = None;
        }
        !finding.file.is_empty() && !finding.summary.is_empty()
    });
    report.findings.truncate(MAX_FINDINGS);
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOOK: &str = include_str!("fixtures/stop-hook.json");
    const BLOCK: &str = r#"{"verdict":"pass","summary":"Fine.","findings":[]}"#;

    #[test]
    fn the_stop_hook_fixture_gives_the_report() {
        let report = from_stop(HOOK).expect("report");
        assert_eq!(report.verdict, Verdict::Concerns);
        assert_eq!(report.summary, "The retry loop never stops on a 4xx reply.");
        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert_eq!(finding.file, "src/upload.rs");
        assert_eq!(finding.line, Some(42));
        assert_eq!(finding.severity, Severity::High);
        assert_eq!(finding.summary, "A 404 is retried forever.");
        assert!(finding.failure_scenario.starts_with("Deleting the bucket"));
    }

    #[test]
    fn a_stop_hook_without_a_reply_is_refused() {
        for input in [
            r#"{"hook_event_name":"Stop"}"#,
            r#"{"last_assistant_message":""}"#,
            r#"{"last_assistant_message":"  \n"}"#,
            r#"{"last_assistant_message":null}"#,
        ] {
            assert_eq!(from_stop(input), Err(NO_REPLY.to_owned()), "{input}");
        }
    }

    #[test]
    fn a_stop_hook_that_is_not_json_is_refused() {
        assert_eq!(from_stop("not json"), Err(UNREADABLE.to_owned()));
        assert_eq!(from_stop(""), Err(UNREADABLE.to_owned()));
    }

    #[test]
    fn one_fenced_block_is_read() {
        let reply = format!("```json\n{BLOCK}\n```");
        assert_eq!(from_reply(&reply).expect("report").verdict, Verdict::Pass);
    }

    #[test]
    fn prose_around_a_fenced_block_is_ignored() {
        let reply = format!("Here you go {{not json}}.\n```json\n{BLOCK}\n```\nThat is all.");
        assert_eq!(from_reply(&reply).expect("report").summary, "Fine.");
    }

    #[test]
    fn the_last_of_two_fenced_blocks_wins() {
        let first = r#"{"verdict":"concerns","summary":"First."}"#;
        let reply = format!("```json\n{first}\n```\nOn reflection:\n```json\n{BLOCK}\n```");
        let report = from_reply(&reply).expect("report");
        assert_eq!(report.summary, "Fine.");
        assert_eq!(report.verdict, Verdict::Pass);
    }

    #[test]
    fn bare_json_and_json_inside_prose_are_read() {
        assert!(from_reply(BLOCK).is_ok());
        assert!(from_reply(&format!("Result: {BLOCK} done")).is_ok());
    }

    #[test]
    fn prose_only_is_refused() {
        assert_eq!(
            from_reply("All good, nothing to report."),
            Err(NOT_FINDINGS.to_owned())
        );
        assert_eq!(from_reply("} before {"), Err(NOT_FINDINGS.to_owned()));
    }

    #[test]
    fn a_block_of_another_shape_is_refused() {
        let reply = "```json\n{\"hello\": 1}\n```";
        assert_eq!(from_reply(reply), Err(NOT_FINDINGS.to_owned()));
        let reply = "```json\n{\"verdict\":\"maybe\"}\n```";
        assert_eq!(from_reply(reply), Err(NOT_FINDINGS.to_owned()));
    }

    #[test]
    fn an_unknown_severity_is_refused() {
        let reply = r#"{"verdict":"concerns","findings":[{"file":"a","severity":"critical","summary":"x"}]}"#;
        assert_eq!(from_reply(reply), Err(NOT_FINDINGS.to_owned()));
    }

    #[test]
    fn missing_summary_and_findings_default() {
        let report = from_reply(r#"{"verdict":"pass"}"#).expect("report");
        assert_eq!(report.summary, "");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn extra_fields_are_ignored() {
        let reply = r#"{"verdict":"pass","mood":"calm","findings":[{"file":"a.rs","severity":"low","summary":"x","extra":1}]}"#;
        let report = from_reply(reply).expect("report");
        assert_eq!(report.findings[0].line, None);
        assert_eq!(report.findings[0].failure_scenario, "");
    }

    #[test]
    fn findings_are_tidied() {
        let reply = r#"{"verdict":"concerns","findings":[
            {"file":" ./src/a.rs ","line":0,"severity":"low","summary":" Odd. "},
            {"file":"src/b.rs","line":3,"severity":"low","summary":"   "},
            {"file":"  ","severity":"low","summary":"No file."}]}"#;
        let report = from_reply(reply).expect("report");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].file, "src/a.rs");
        assert_eq!(report.findings[0].line, None);
        assert_eq!(report.findings[0].summary, "Odd.");
    }

    #[test]
    fn findings_are_cut_to_the_maximum() {
        let one = r#"{"file":"a.rs","line":1,"severity":"low","summary":"x"}"#;
        let many = vec![one; 150].join(",");
        let reply = format!(r#"{{"verdict":"concerns","findings":[{many}]}}"#);
        assert_eq!(
            from_reply(&reply).expect("report").findings.len(),
            MAX_FINDINGS
        );
    }
}
