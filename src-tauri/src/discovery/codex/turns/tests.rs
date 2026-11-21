//! Section 8.2 of the M2a spec: rollout records composed from the Codex source's record shapes,
//! with placeholder text only.

use super::super::{Transcript, parse_transcript, signals};
use super::*;
use std::io::Cursor;

fn parse(lines: &[String]) -> Transcript {
    parse_transcript(Cursor::new(lines.join("\n")))
}

fn event(kind: &str, extra: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-11-20T10:00:00.000Z","type":"event_msg","payload":{{"type":"{kind}"{extra}}}}}"#
    )
}

fn item(payload: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-11-20T10:00:00.000Z","type":"response_item","payload":{payload}}}"#
    )
}

fn started(id: &str) -> String {
    event("task_started", &format!(r#","turn_id":"{id}""#))
}

fn complete(id: &str) -> String {
    event("task_complete", &format!(r#","turn_id":"{id}""#))
}

fn call(id: &str, name: &str, arguments: &str) -> String {
    item(&format!(
        r#"{{"type":"function_call","call_id":"{id}","name":"{name}","arguments":{}}}"#,
        serde_json::to_string(arguments).expect("a string serialises")
    ))
}

fn output(id: &str) -> String {
    item(&format!(
        r#"{{"type":"function_call_output","call_id":"{id}","output":"OUTPUT"}}"#
    ))
}

fn message(role: &str, phase: Option<&str>, text: &str) -> String {
    let phase = phase.map_or(String::new(), |p| format!(r#","phase":"{p}""#));
    item(&format!(
        r#"{{"type":"message","role":"{role}"{phase},"content":[{{"type":"output_text","text":"{text}"}}]}}"#
    ))
}

const TWO_QUESTIONS: &str = r#"{"questions":[{"id":"a","header":"H","question":"QUESTION-ONE","isOther":true,"isSecret":false,"options":[{"label":"OPTION-A","description":"D"},{"label":"OPTION-B","description":"D"}]},{"id":"b","header":"H","question":"QUESTION-TWO","options":[]}]}"#;

#[test]
fn an_aborted_turn_leaves_no_open_tool() {
    let t = parse(&[
        started("t1"),
        call("c1", "tool_x", "{}"),
        event("turn_aborted", r#","turn_id":"t1""#),
    ]);
    assert_eq!(t.open_tools, 0);
    assert!(!t.turn_open);
    assert!(!t.api_error);
}

#[test]
fn a_failed_turn_leaves_no_open_tool_and_is_an_error() {
    let failed = event(
        "task_complete",
        r#","turn_id":"t1","error":{"message":"M"}"#,
    );
    let t = parse(&[started("t1"), call("c1", "tool_x", "{}"), failed]);
    assert_eq!(t.open_tools, 0);
    assert!(t.api_error);
    assert!(!t.turn_open);
}

#[test]
fn the_turn_aliases_read_like_the_task_names() {
    let t = parse(&[
        event("turn_started", r#","turn_id":"t1""#),
        call("c1", "tool_x", "{}"),
    ]);
    assert!(t.turn_open);
    assert_eq!(t.open_tools, 1);
    let t = parse(&[
        event("turn_started", r#","turn_id":"t1""#),
        call("c1", "tool_x", "{}"),
        event("turn_complete", r#","turn_id":"t1""#),
    ]);
    assert!(!t.turn_open);
    assert_eq!(t.open_tools, 0);
}

#[test]
fn a_late_record_of_an_older_turn_changes_nothing_but_the_ended_list() {
    let t = parse(&[
        started("t1"),
        complete("t1"),
        started("t2"),
        call("c1", "tool_x", "{}"),
        complete("t1"),
    ]);
    assert!(t.turn_open);
    assert_eq!(t.open_tools, 1);
    assert_eq!(t.evidence.turn_id.as_deref(), Some("t2"));
    // A late start of an ended turn does not reopen it either.
    let t = parse(&[started("t1"), complete("t1"), started("t2"), started("t1")]);
    assert_eq!(t.evidence.turn_id.as_deref(), Some("t2"));
}

#[test]
fn an_open_request_user_input_call_is_a_formal_question() {
    let t = parse(&[started("t1"), call("c1", ASK_TOOL, TWO_QUESTIONS)]);
    assert_eq!(
        t.asks,
        vec![Ask {
            call_id: "c1".into(),
            prompt: "QUESTION-ONE".into(),
            options: vec!["OPTION-A".into(), "OPTION-B".into()],
            questions: 2,
            asked_at: "2026-11-20T10:00:00.000Z".into(),
        }]
    );
    assert!(signals(&t, Some(1)).question_waiting);
}

#[test]
fn the_ask_outlasts_other_records_until_its_output() {
    let mut lines = vec![
        started("t1"),
        call("c1", ASK_TOOL, TWO_QUESTIONS),
        event("token_count", ""),
        call("c2", "tool_x", "{}"),
        output("c2"),
    ];
    let t = parse(&lines);
    assert_eq!(t.asks.len(), 1);
    assert_eq!(t.open_tools, 1);

    lines.push(output("c1"));
    let t = parse(&lines);
    assert!(t.asks.is_empty());
    assert!(!signals(&t, Some(1)).question_waiting);
    assert!(t.evidence.done_calls.contains(&"c1".to_string()));
}

#[test]
fn arguments_that_are_not_json_still_make_an_ask() {
    for arguments in ["not json", "{}", r#"{"questions":[]}"#] {
        let t = parse(&[started("t1"), call("c1", ASK_TOOL, arguments)]);
        let [ask] = t.asks.as_slice() else {
            panic!("expected one ask for {arguments:?}");
        };
        assert_eq!(ask.prompt, "Codex asked a question in its terminal");
        assert!(ask.options.is_empty());
        assert_eq!(ask.questions, 0);
        assert_eq!(t.open_tools, 1);
    }
}

#[test]
fn an_ask_is_cut_to_its_limits() {
    let labels: Vec<String> = (0..8)
        .map(|i| format!(r#"{{"label":"{}"}}"#, format!("{i}").repeat(200)))
        .collect();
    let arguments = format!(
        r#"{{"questions":[{{"question":"{}","options":[{}]}}]}}"#,
        "Q".repeat(500),
        labels.join(",")
    );
    let t = parse(&[started("t1"), call("c1", ASK_TOOL, &arguments)]);
    let [ask] = t.asks.as_slice() else {
        panic!("one ask");
    };
    assert_eq!(ask.prompt.chars().count(), 400);
    assert_eq!(ask.options.len(), 6);
    assert!(ask.options.iter().all(|o| o.chars().count() == 120));
}

#[test]
fn a_question_in_commentary_followed_by_a_plain_final_answer_is_no_ask() {
    let t = parse(&[
        started("t1"),
        message("assistant", Some("commentary"), "Shall I go on?"),
        message("assistant", Some("final_answer"), "All done."),
    ]);
    assert_eq!(t.reply_to_human, "All done.");
    // Commentary alone never becomes the reply.
    let t = parse(&[
        started("t1"),
        message("assistant", Some("final_answer"), "All done."),
        message("assistant", Some("commentary"), "Shall I go on?"),
    ]);
    assert_eq!(t.reply_to_human, "All done.");
}

#[test]
fn the_reply_is_the_last_message_not_the_two_joined() {
    let t = parse(&[
        started("t1"),
        message("assistant", None, "FIRST"),
        message("assistant", None, "SECOND"),
    ]);
    assert_eq!(t.reply_to_human, "SECOND");
    assert!(!t.human_spoke_last);
}

#[test]
fn a_human_message_in_any_encoding_empties_the_reply() {
    let encodings = [
        event("user_message", r#","message":"MSG""#),
        event(
            "item_completed",
            r#","item":{"type":"UserMessage","id":"i"}"#,
        ),
        message("user", None, "MSG"),
    ];
    for human in encodings {
        let t = parse(&[started("t1"), message("assistant", None, "REPLY"), human]);
        assert!(t.human_spoke_last);
        assert!(t.reply_to_human.is_empty());
    }
    let other = event("item_completed", r#","item":{"type":"AgentMessage"}"#);
    assert!(!parse(&[other]).human_spoke_last);
}

#[test]
fn evidence_names_the_turn_and_what_ended() {
    let mut lines = Vec::new();
    for i in 0..10 {
        lines.push(started(&format!("t{i}")));
        lines.push(complete(&format!("t{i}")));
    }
    lines.push(started("t10"));
    lines.push(call("c1", "tool_x", "{}"));
    lines.push(output("c1"));
    let e = parse(&lines).evidence;
    assert_eq!(e.turn_id.as_deref(), Some("t10"));
    assert_eq!(e.ended_turns.len(), 8);
    assert_eq!(e.ended_turns.first().map(String::as_str), Some("t2"));
    assert_eq!(e.ended_turns.last().map(String::as_str), Some("t9"));
    assert_eq!(e.done_calls, vec!["c1".to_string()]);

    lines.push(started("t11"));
    let e = parse(&lines).evidence;
    assert!(e.done_calls.is_empty());
    assert_eq!(e.turn_id.as_deref(), Some("t11"));
}

#[test]
fn done_calls_keep_the_newest_64() {
    let mut lines = vec![started("t1")];
    for i in 0..70 {
        lines.push(call(&format!("c{i}"), "tool_x", "{}"));
        lines.push(output(&format!("c{i}")));
    }
    let done = parse(&lines).evidence.done_calls;
    assert_eq!(done.len(), 64);
    assert_eq!(done.first().map(String::as_str), Some("c6"));
}

// Moved from `codex.rs` with the reducer (section 5.1).
const META: &str = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"s1","session_id":"root1","cwd":"/code/app","source":"cli"}}"#;
const USER: &str = r#"{"timestamp":"2026-09-29T10:00:02.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix the parser"}]}}"#;

fn parse_str(lines: &[&str]) -> Transcript {
    parse_transcript(Cursor::new(lines.join("\n")))
}

#[test]
fn turn_in_progress_until_task_complete() {
    let started = r#"{"timestamp":"2026-09-29T10:00:03.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
    let t = parse_str(&[META, USER, started]);
    assert!(t.turn_open);

    let complete = r#"{"timestamp":"2026-09-29T10:00:06.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}"#;
    let done = parse_str(&[META, USER, started, complete]);
    assert!(!done.turn_open);
    assert!(!done.api_error);
}

#[test]
fn task_complete_error_is_flagged() {
    let complete = r#"{"timestamp":"2026-09-29T10:00:06.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","error":{"message":"usage limit","codex_error_info":"usage_limit_exceeded"}}}"#;
    let t = parse_str(&[META, USER, complete]);
    assert!(t.api_error);
}

#[test]
fn open_function_call_counts_as_pending() {
    let call = r#"{"timestamp":"2026-09-29T10:00:03.000Z","type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell","arguments":"{}"}}"#;
    let t = parse_str(&[META, USER, call]);
    assert_eq!(t.open_tools, 1);
    assert_eq!(t.last_block, crate::discovery::rules::LastBlock::ToolUse);

    let output = r#"{"timestamp":"2026-09-29T10:00:04.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"ok"}}"#;
    let closed = parse_str(&[META, USER, call, output]);
    assert_eq!(closed.open_tools, 0);
}

// VERIFIER (failing on purpose): Codex writes `turn_aborted` (no `task_complete`) when the
// user interrupts a turn. 41/41 aborts in real rollouts on this Mac had no later task_complete.
#[test]
fn an_interrupted_turn_is_over() {
    let started = r#"{"timestamp":"2026-09-29T10:00:03.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
    let aborted = r#"{"timestamp":"2026-09-29T10:00:09.000Z","type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t1","reason":"interrupted"}}"#;
    let t = parse_str(&[META, USER, started, aborted]);
    assert!(
        !t.turn_open,
        "an interrupted turn is over; live session would show Running forever"
    );
}
