//! The one pull request review the building posts for a report.

use serde::Serialize;

use super::diff::ChangedLines;
use super::findings::{Finding, Report, Severity, Verdict};

/// The body of `gh api .../reviews --input`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewPayload {
    pub commit_id: String,
    pub event: Event,
    pub body: String,
    pub comments: Vec<InlineComment>,
}

/// The review event. Only `COMMENT` exists, so no other can be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Event {
    #[serde(rename = "COMMENT")]
    Comment,
}

/// A comment on one line of the pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InlineComment {
    pub path: String,
    pub line: u32,
    pub side: Side,
    pub body: String,
}

/// The side of the diff. Only the new side exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Side {
    #[serde(rename = "RIGHT")]
    Right,
}

pub const HEADER: &str = "Office Building Quality Assurance";
pub const MAX_INLINE: usize = 25;
pub const BODY_MAX: usize = 60_000;

const SUMMARY_MAX: usize = 2000;
const FINDING_MAX: usize = 600;
const SCENARIO_MAX: usize = 1200;
const FILE_MAX: usize = 300;
/// Room kept for the closing "and N more" line.
const MORE_ROOM: usize = 40;
const ZERO_WIDTH_SPACE: char = '\u{200b}';

/// Where findings on changed lines go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// As inline comments, where the diff allows.
    Inline,
    /// All in the summary (a retry after GitHub refused the inline form).
    Summary,
}

/// The one review for `report` on `head`.
#[must_use]
pub fn build(
    report: &Report,
    lines: &ChangedLines,
    head: &str,
    placement: Placement,
) -> ReviewPayload {
    let mut comments = Vec::new();
    let mut listed = Vec::new();
    for finding in &report.findings {
        let inline = placement == Placement::Inline
            && comments.len() < MAX_INLINE
            && finding
                .line
                .is_some_and(|n| lines.contains(&finding.file, n));
        match finding.line {
            Some(line) if inline => comments.push(inline_comment(finding, line)),
            _ => listed.push(finding),
        }
    }
    let sha: String = head.chars().take(7).collect();
    let footer = format!(
        "Reviewed commit {sha}. Written by a review agent in Office Building and posted from \
         this account. Label a pull request no-qa to skip it."
    );
    let summary = clean(&report.summary, SUMMARY_MAX);
    let mut top = if report.findings.is_empty() {
        format!("{HEADER}\n\nNo issues found.")
    } else {
        let verdict = match report.verdict {
            Verdict::Concerns => "problems found",
            Verdict::Pass => "no blocking problems found",
        };
        format!("{HEADER}\n\nVerdict: {verdict}.")
    };
    if !summary.is_empty() {
        top.push('\n');
        top.push_str(&summary);
    }
    let heading = match placement {
        Placement::Inline => "Findings outside the changed lines:",
        Placement::Summary => "Findings:",
    };
    let list = list_block(
        heading,
        &listed,
        BODY_MAX.saturating_sub(top.len() + footer.len()),
    );
    ReviewPayload {
        commit_id: head.to_owned(),
        event: Event::Comment,
        body: format!("{top}\n\n{list}{footer}"),
        comments,
    }
}

fn inline_comment(finding: &Finding, line: u32) -> InlineComment {
    let mut body = format!(
        "{}: {}",
        severity_word(finding.severity),
        clean(&finding.summary, FINDING_MAX)
    );
    let scenario = clean(&finding.failure_scenario, SCENARIO_MAX);
    if !scenario.is_empty() {
        body.push_str("\n\nFailure scenario: ");
        body.push_str(&scenario);
    }
    InlineComment {
        // The path is matched against the diff, so it goes as the diff spells it.
        path: finding.file.clone(),
        line,
        side: Side::Right,
        body,
    }
}

/// The heading and list lines, each followed by a blank line, within `room` characters.
/// Empty when nothing is listed.
fn list_block(heading: &str, listed: &[&Finding], room: usize) -> String {
    if listed.is_empty() {
        return String::new();
    }
    let mut block = format!("{heading}\n");
    let limit = room.saturating_sub(MORE_ROOM);
    for (index, finding) in listed.iter().enumerate() {
        let line = list_line(finding);
        if block.len() + line.len() > limit {
            block.push_str("- and ");
            block.push_str(&(listed.len() - index).to_string());
            block.push_str(" more\n");
            break;
        }
        block.push_str(&line);
    }
    block.push('\n');
    block
}

fn list_line(finding: &Finding) -> String {
    let file = clean(&finding.file, FILE_MAX);
    let place = match finding.line {
        Some(line) => format!("`{file}:{line}`"),
        None => format!("`{file}`"),
    };
    let summary = clean(&finding.summary, FINDING_MAX).replace('\n', " ");
    let mut line = format!(
        "- {place} ({}): {summary}",
        severity_word(finding.severity).to_lowercase()
    );
    let scenario = clean(&finding.failure_scenario, SCENARIO_MAX).replace('\n', " ");
    if !scenario.is_empty() {
        line.push_str(" Failure scenario: ");
        line.push_str(&scenario);
    }
    line.push('\n');
    line
}

fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::High => "High",
        Severity::Medium => "Medium",
        Severity::Low => "Low",
    }
}

/// Agent text made safe to post: no control characters but newlines, trimmed, cut to `max`
/// characters (ending in `…`), and a zero-width space after every `@` so it cannot mention anyone.
fn clean(text: &str, max: usize) -> String {
    let kept: String = text
        .chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect();
    let kept = kept.trim();
    let cut: String = if kept.chars().count() > max {
        kept.chars()
            .take(max.saturating_sub(1))
            .chain(['…'])
            .collect()
    } else {
        kept.to_owned()
    };
    cut.replace('@', &format!("@{ZERO_WIDTH_SPACE}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa::findings::from_reply;

    const DIFF: &str = include_str!("fixtures/pull-request.diff");
    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
    const FOOTER: &str = "Reviewed commit 0123456. Written by a review agent in Office Building and posted from this account. Label a pull request no-qa to skip it.";

    fn finding(file: &str, line: Option<u32>, summary: &str, scenario: &str) -> Finding {
        Finding {
            file: file.to_owned(),
            line,
            severity: Severity::High,
            summary: summary.to_owned(),
            failure_scenario: scenario.to_owned(),
        }
    }

    fn report(verdict: Verdict, summary: &str, findings: Vec<Finding>) -> Report {
        Report {
            verdict,
            summary: summary.to_owned(),
            findings,
        }
    }

    fn lines() -> ChangedLines {
        ChangedLines::parse(DIFF)
    }

    #[test]
    fn a_finding_on_a_changed_line_is_inline() {
        let r = report(
            Verdict::Concerns,
            "Needs work.",
            vec![finding(
                "src/upload.rs",
                Some(12),
                "Wrong value.",
                "Input 0 breaks it.",
            )],
        );
        let payload = build(&r, &lines(), HEAD, Placement::Inline);
        assert_eq!(payload.commit_id, HEAD);
        assert_eq!(
            payload.comments,
            [InlineComment {
                path: "src/upload.rs".to_owned(),
                line: 12,
                side: Side::Right,
                body: "High: Wrong value.\n\nFailure scenario: Input 0 breaks it.".to_owned(),
            }]
        );
        assert!(!payload.body.contains("outside the changed lines"));
        assert_eq!(
            payload.body,
            format!("{HEADER}\n\nVerdict: problems found.\nNeeds work.\n\n{FOOTER}")
        );
    }

    #[test]
    fn an_inline_path_is_sent_as_the_diff_spells_it() {
        let diff = "diff --git a/packages/@acme/ui/button.tsx b/packages/@acme/ui/button.tsx\n\
                    --- a/packages/@acme/ui/button.tsx\n+++ b/packages/@acme/ui/button.tsx\n\
                    @@ -1,1 +1,2 @@\n keep\n+added\n";
        let r = report(
            Verdict::Concerns,
            "",
            vec![finding("packages/@acme/ui/button.tsx", Some(2), "Odd.", "")],
        );
        let payload = build(&r, &ChangedLines::parse(diff), HEAD, Placement::Inline);
        assert_eq!(payload.comments[0].path, "packages/@acme/ui/button.tsx");
    }

    #[test]
    fn an_inline_comment_without_a_scenario_is_one_line() {
        let r = report(
            Verdict::Concerns,
            "",
            vec![finding("src/upload.rs", Some(12), "Odd.", "")],
        );
        let payload = build(&r, &lines(), HEAD, Placement::Inline);
        assert_eq!(payload.comments[0].body, "High: Odd.");
    }

    #[test]
    fn other_findings_go_to_the_list() {
        let mut low = finding("README.md", None, "Stale text.", "");
        low.severity = Severity::Low;
        let r = report(
            Verdict::Concerns,
            "Two things.",
            vec![
                finding("src/upload.rs", Some(30), "Unchanged line.", "It loops."),
                low,
                finding("src/elsewhere.rs", Some(1), "Not in the diff.", ""),
            ],
        );
        let payload = build(&r, &lines(), HEAD, Placement::Inline);
        assert!(payload.comments.is_empty());
        assert_eq!(
            payload.body,
            format!(
                "{HEADER}\n\nVerdict: problems found.\nTwo things.\n\n\
                 Findings outside the changed lines:\n\
                 - `src/upload.rs:30` (high): Unchanged line. Failure scenario: It loops.\n\
                 - `README.md` (low): Stale text.\n\
                 - `src/elsewhere.rs:1` (high): Not in the diff.\n\n{FOOTER}"
            )
        );
    }

    #[test]
    fn summary_placement_makes_no_inline_comments() {
        let r = report(
            Verdict::Concerns,
            "",
            vec![finding("src/upload.rs", Some(12), "Wrong value.", "")],
        );
        let payload = build(&r, &lines(), HEAD, Placement::Summary);
        assert!(payload.comments.is_empty());
        assert!(
            payload
                .body
                .contains("\nFindings:\n- `src/upload.rs:12` (high): Wrong value.\n")
        );
        assert!(!payload.body.contains("outside the changed lines"));
    }

    #[test]
    fn the_pass_verdict_has_its_own_line() {
        let r = report(Verdict::Pass, "", vec![finding("a.rs", None, "Nit.", "")]);
        let payload = build(&r, &lines(), HEAD, Placement::Inline);
        assert!(
            payload
                .body
                .contains("Verdict: no blocking problems found.\n\nFindings")
        );
    }

    #[test]
    fn the_twenty_sixth_inline_finding_goes_to_the_list() {
        let many: Vec<Finding> = (0..26)
            .map(|n| finding("src/upload.rs", Some(12), &format!("Issue {n}."), ""))
            .collect();
        let payload = build(
            &report(Verdict::Concerns, "", many),
            &lines(),
            HEAD,
            Placement::Inline,
        );
        assert_eq!(payload.comments.len(), MAX_INLINE);
        assert!(
            payload
                .body
                .contains("- `src/upload.rs:12` (high): Issue 25.")
        );
        assert!(!payload.body.contains("Issue 24."));
    }

    #[test]
    fn mentions_are_defused_in_the_summary_and_in_comments() {
        let r = report(
            Verdict::Concerns,
            "Ask @octo-dev.",
            vec![finding(
                "src/upload.rs",
                Some(12),
                "Ping @octo-dev now.",
                "Then @octo-dev wakes.",
            )],
        );
        let payload = build(&r, &lines(), HEAD, Placement::Inline);
        assert!(payload.body.contains("Ask @\u{200b}octo-dev."));
        assert!(!payload.body.contains("@octo"));
        assert_eq!(
            payload.comments[0].body,
            "High: Ping @\u{200b}octo-dev now.\n\nFailure scenario: Then @\u{200b}octo-dev wakes."
        );
    }

    #[test]
    fn control_characters_go_but_newlines_stay() {
        assert_eq!(clean("a\u{7}b\u{1b}[0m\r\nc\td", 100), "ab[0m\ncd");
        assert_eq!(clean("  padded \n", 100), "padded");
    }

    #[test]
    fn long_texts_are_cut_with_an_ellipsis() {
        let cut = clean(&"x".repeat(50), 10);
        assert_eq!(cut, format!("{}…", "x".repeat(9)));
        assert_eq!(clean(&"y".repeat(10), 10), "y".repeat(10));
        let r = report(Verdict::Concerns, &"s".repeat(5000), vec![]);
        let payload = build(&r, &lines(), HEAD, Placement::Inline);
        assert!(
            payload
                .body
                .contains(&format!("{}…\n", "s".repeat(SUMMARY_MAX - 1)))
        );
    }

    #[test]
    fn no_findings_gives_exactly_the_clean_text() {
        let r = report(Verdict::Pass, "The change is small and tidy.", vec![]);
        let payload = build(&r, &lines(), HEAD, Placement::Inline);
        assert_eq!(
            payload.body,
            "Office Building Quality Assurance\n\nNo issues found.\nThe change is small and tidy.\n\n\
             Reviewed commit 0123456. Written by a review agent in Office Building and posted \
             from this account. Label a pull request no-qa to skip it."
        );
        assert!(payload.comments.is_empty());
        let empty = build(
            &report(Verdict::Pass, "", vec![]),
            &lines(),
            HEAD,
            Placement::Inline,
        );
        assert_eq!(
            empty.body,
            format!("{HEADER}\n\nNo issues found.\n\n{FOOTER}")
        );
    }

    #[test]
    fn the_payload_serialises_with_comment_and_right() {
        let r = report(
            Verdict::Concerns,
            "",
            vec![finding("src/upload.rs", Some(12), "Odd.", "")],
        );
        let json =
            serde_json::to_string(&build(&r, &lines(), HEAD, Placement::Inline)).expect("json");
        assert!(json.contains(r#""event":"COMMENT""#));
        assert!(json.contains(r#""side":"RIGHT""#));
        assert!(json.contains(&format!(r#""commit_id":"{HEAD}""#)));
    }

    #[test]
    fn a_hundred_long_findings_stay_within_the_body_limit() {
        let many: Vec<Finding> = (0..100)
            .map(|n| {
                finding(
                    "README.md",
                    None,
                    &format!("{n} {}", "w".repeat(590)),
                    &"v".repeat(1200),
                )
            })
            .collect();
        let r = report(Verdict::Concerns, &"s".repeat(2000), many);
        let payload = build(&r, &lines(), HEAD, Placement::Inline);
        assert!(payload.body.chars().count() <= BODY_MAX);
        let (listing, tail) = payload.body.rsplit_once("\n\n").expect("footer");
        assert_eq!(tail, FOOTER);
        let last = listing.lines().last().expect("last line");
        assert!(
            last.starts_with("- and ") && last.ends_with(" more"),
            "{last}"
        );
    }

    #[test]
    fn a_report_read_from_a_reply_builds() {
        let reply = r#"{"verdict":"pass","findings":[{"file":"src/upload.rs","line":42,"severity":"low","summary":"x"}]}"#;
        let report = from_reply(reply).expect("report");
        let payload = build(&report, &lines(), HEAD, Placement::Inline);
        assert_eq!(payload.comments.len(), 1);
    }
}
