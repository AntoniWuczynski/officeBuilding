//! Every request kept under its thread and turn, answered once and retired by
//! its owner (build spec M2a, sections 4.4 to 4.6, table 8.1). Every line below
//! was composed from the bundle `codex app-server generate-json-schema` writes
//! for codex-cli 0.161.0 (`ToolRequestUserInputParams.json`,
//! `CommandExecutionRequestApprovalParams.json`, `McpServerElicitationRequestParams.json`,
//! `v2/TurnStartedNotification.json`, `v2/TurnCompletedNotification.json`,
//! `v2/ServerRequestResolvedNotification.json`, `v2/ThreadStatusChangedNotification.json`,
//! `v2/ThreadResumeResponse.json`), with `rootTurnId` from rust-v0.170.0. None is replayed.

use super::mapper::{Mapper, Out};
use super::tests::running_on;
use crate::model::SessionState;
use crate::structured::{Entry, Event, ToolStatus, Values};

/// The desk's own thread, a child thread, and two turns of the desk's thread.
const T: &str = "00000000-0000-0000-0000-000000000001";
const C: &str = "00000000-0000-0000-0000-000000000002";
const X: &str = "00000000-0000-0000-0000-000000000011";
const Y: &str = "00000000-0000-0000-0000-000000000012";

/// One thing A Human or Codex does.
enum Step {
    /// A line from Codex.
    Line(String),
    /// A message from A Human.
    Prompt(&'static str),
    /// An answer, with the values typed and whether its write works.
    Answer {
        id: &'static str,
        choice: &'static str,
        values: &'static [(&'static str, &'static str)],
        works: bool,
    },
}

/// What A Human would see after one step.
#[derive(Debug, Default, PartialEq, Eq)]
struct Seen {
    /// Lines written to Codex.
    written: Vec<String>,
    /// Approvals opened or changed, as their id with what marks them.
    opened: Vec<String>,
    resolved: Vec<String>,
    tools: Vec<(String, ToolStatus)>,
    /// The last state the step reported.
    state: Option<SessionState>,
    /// The answer's error.
    error: Option<String>,
}

fn take(seen: &mut Seen, out: Vec<Out>) {
    for o in out {
        match o {
            Out::Send(line) => seen.written.push(line),
            Out::Event(Event::Approval(a)) => {
                let mut mark = a.id;
                if a.background {
                    mark.push_str(" background");
                }
                if a.failed.is_some() {
                    mark.push_str(" failed");
                }
                if a.choices.is_empty() {
                    mark.push_str(" no choices");
                }
                seen.opened.push(mark);
            }
            Out::Event(Event::Resolved { approval_id }) => seen.resolved.push(approval_id),
            Out::Event(Event::Entry(Entry::Tool { id, status, .. })) => {
                seen.tools.push((id, status));
            }
            Out::Event(Event::State(s)) => seen.state = Some(s),
            Out::Event(_) => {}
        }
    }
}

fn run(m: &mut Mapper, step: &Step) -> Seen {
    let mut seen = Seen::default();
    match step {
        Step::Line(line) => take(&mut seen, m.line(line)),
        Step::Prompt(text) => take(&mut seen, m.prompt(text)),
        Step::Answer {
            id,
            choice,
            values,
            works,
        } => {
            let values: Values = values
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect();
            let mut written = Vec::new();
            let (out, result) = m.answer(id, choice, &values, |line| {
                if *works {
                    written.push(line.to_string());
                    Ok(())
                } else {
                    Err("PIPE-CLOSED".to_string())
                }
            });
            seen.written = written;
            take(&mut seen, out);
            seen.error = result.err();
        }
    }
    seen
}

/// Feed every step and check what A Human sees after each.
fn walk(m: &mut Mapper, table: Vec<(Step, Seen)>) {
    for (i, (step, want)) in table.into_iter().enumerate() {
        assert_eq!(run(m, &step), want, "step {i}");
    }
}

/// A desk on thread `T` with turn `X` running.
fn desk() -> Mapper {
    running_on(T, X, "TASK-ONE")
}

fn line(s: String) -> Step {
    Step::Line(s)
}

fn answer(id: &'static str, choice: &'static str) -> Step {
    Step::Answer {
        id,
        choice,
        values: &[],
        works: true,
    }
}

fn failing(id: &'static str, choice: &'static str) -> Step {
    Step::Answer {
        id,
        choice,
        values: &[],
        works: false,
    }
}

/// A two-question form from `thread` in `turn`, blocking as `blocking` says
/// (`None` leaves the field out).
fn questions(id: &str, thread: &str, turn: &str, blocking: Option<bool>) -> String {
    let blocking = blocking.map_or(String::new(), |b| format!(r#""isBlocking":{b},"#));
    format!(
        r#"{{"id":{id},"method":"item/tool/requestUserInput","params":{{"threadId":"{thread}","turnId":"{turn}","itemId":"item-q",{blocking}"questions":[{{"id":"q-one","header":"HEADER-ONE","question":"QUESTION-ONE","isOther":false,"isSecret":false,"options":[{{"label":"OPTION-A","description":""}},{{"label":"OPTION-B","description":""}}]}},{{"id":"q-two","header":"","question":"QUESTION-TWO","options":null}}]}}}}"#
    )
}

/// A command approval from `thread` in `turn`, with no decision list (so the
/// defaults: accept, accept for the session, decline, cancel).
fn command(id: &str, thread: &str, turn: &str) -> String {
    format!(
        r#"{{"id":{id},"method":"item/commandExecution/requestApproval","params":{{"threadId":"{thread}","turnId":"{turn}","itemId":"item-c","startedAtMs":1,"command":"tool-x --flag","cwd":"/w"}}}}"#
    )
}

fn resolved_note(id: &str) -> String {
    format!(
        r#"{{"method":"serverRequest/resolved","params":{{"threadId":"{T}","requestId":{id}}}}}"#
    )
}

fn turn_note(method: &str, thread: &str, turn: &str, status: &str) -> String {
    format!(
        r#"{{"method":"turn/{method}","params":{{"threadId":"{thread}","turn":{{"id":"{turn}","items":[],"status":"{status}","error":null}}}}}}"#
    )
}

fn completed(thread: &str, turn: &str) -> String {
    turn_note("completed", thread, turn, "completed")
}

fn status(thread: &str, status: &str) -> String {
    format!(
        r#"{{"method":"thread/status/changed","params":{{"threadId":"{thread}","status":{status}}}}}"#
    )
}

fn delta() -> String {
    format!(
        r#"{{"method":"item/agentMessage/delta","params":{{"threadId":"{T}","turnId":"{X}","itemId":"item-m","delta":"TEXT"}}}}"#
    )
}

fn exec_started(status: &str) -> String {
    format!(
        r#"{{"method":"item/started","params":{{"threadId":"{T}","turnId":"{X}","item":{{"type":"commandExecution","id":"item-c","command":"tool-x --flag","cwd":"/w","status":"{status}","commandActions":[],"aggregatedOutput":null,"exitCode":null}}}}}}"#
    )
}

fn decision(id: &str, decision: &str) -> String {
    format!(r#"{{"id":{id},"result":{{"decision":"{decision}"}}}}"#)
}

fn opened(ids: &[&str], state: SessionState) -> Seen {
    Seen {
        opened: ids.iter().map(|s| (*s).to_string()).collect(),
        state: Some(state),
        ..Seen::default()
    }
}

fn gone(ids: &[&str], state: SessionState) -> Seen {
    Seen {
        resolved: ids.iter().map(|s| (*s).to_string()).collect(),
        state: Some(state),
        ..Seen::default()
    }
}

fn written(lines: &[String], resolved: &[&str], state: SessionState) -> Seen {
    Seen {
        written: lines.to_vec(),
        ..gone(resolved, state)
    }
}

const NOT_OPEN: &str = "that request is no longer open";

/// Rows 1 to 6: overlapping requests, each answered or withdrawn on its own,
/// and an answer whose write failed.
#[test]
fn overlapping_requests_are_answered_one_by_one() {
    use SessionState::{Thinking, WaitingHuman};
    let mut m = desk();
    walk(
        &mut m,
        vec![
            // 1
            (line(questions("1", T, X, Some(true))), opened(&["1"], WaitingHuman)),
            (line(command("2", T, X)), opened(&["2"], WaitingHuman)),
            // 2
            (answer("2", "0"), written(&[decision("2", "accept")], &["2"], WaitingHuman)),
            // 3
            (
                Step::Answer {
                    id: "1",
                    choice: "0",
                    values: &[("q-one", "OPTION-A"), ("q-two", "ANSWER-TWO")],
                    works: true,
                },
                written(
                    &[r#"{"id":1,"result":{"answers":{"q-one":{"answers":["OPTION-A"]},"q-two":{"answers":["ANSWER-TWO"]}}}}"#.to_string()],
                    &["1"],
                    Thinking,
                ),
            ),
            // 4: a withdrawal by the server retires that one only.
            (line(questions("3", T, X, Some(true))), opened(&["3"], WaitingHuman)),
            (line(command("4", T, X)), opened(&["4"], WaitingHuman)),
            (line(resolved_note("3")), gone(&["3"], WaitingHuman)),
            // 5: a failed write keeps the request, saying why, and it can be sent again.
            (
                failing("4", "2"),
                Seen {
                    opened: vec!["4 failed".to_string()],
                    state: Some(SessionState::WaitingHuman),
                    error: Some("PIPE-CLOSED".to_string()),
                    ..Seen::default()
                },
            ),
            (answer("4", "2"), written(&[decision("4", "decline")], &["4"], Thinking)),
            // 6: after a failed write, the server's resolution retires it.
            (line(command("5", T, X)), opened(&["5"], WaitingHuman)),
            (
                failing("5", "0"),
                Seen {
                    opened: vec!["5 failed".to_string()],
                    state: Some(SessionState::WaitingHuman),
                    error: Some("PIPE-CLOSED".to_string()),
                    ..Seen::default()
                },
            ),
            (line(resolved_note("5")), gone(&["5"], Thinking)),
            (
                answer("5", "0"),
                Seen {
                    error: Some(NOT_OPEN.to_string()),
                    ..Seen::default()
                },
            ),
        ],
    );
}

/// Row 7: a non-blocking question while a command runs leaves the desk running.
#[test]
fn a_background_question_leaves_a_running_desk_running() {
    use SessionState::Running;
    let mut m = desk();
    walk(
        &mut m,
        vec![
            (
                line(exec_started("inProgress")),
                Seen {
                    tools: vec![("item-c".to_string(), ToolStatus::Running)],
                    state: Some(SessionState::Running),
                    ..Seen::default()
                },
            ),
            (line(questions("6", T, X, Some(false))), opened(&["6 background"], Running)),
            (
                Step::Answer {
                    id: "6",
                    choice: "0",
                    values: &[("q-one", "OPTION-B"), ("q-two", "ANSWER-TWO")],
                    works: true,
                },
                written(
                    &[r#"{"id":6,"result":{"answers":{"q-one":{"answers":["OPTION-B"]},"q-two":{"answers":["ANSWER-TWO"]}}}}"#.to_string()],
                    &["6"],
                    Running,
                ),
            ),
        ],
    );
}

/// Row 8: a question that does not say whether it blocks, blocks.
#[test]
fn a_question_with_no_blocking_flag_blocks() {
    walk(
        &mut desk(),
        vec![(
            line(questions("7", T, X, None)),
            opened(&["7"], SessionState::WaitingHuman),
        )],
    );
}

/// Rows 9 to 14: requests retired by the turn and the thread that own them.
#[test]
fn a_turn_retires_only_the_requests_it_owns() {
    use SessionState::{Idle, Thinking, WaitingHuman};
    let elicitation = format!(
        r#"{{"id":11,"method":"mcpServer/elicitation/request","params":{{"threadId":"{T}","turnId":null,"serverName":"server-x","mode":"form","_meta":null,"message":"MESSAGE-ONE","requestedSchema":{{"type":"object","properties":{{"field-a":{{"type":"string"}}}},"required":null}}}}}}"#
    );
    let mut m = desk();
    walk(
        &mut m,
        vec![
            // 9: the turn's end retires its request, and a later answer writes nothing.
            (line(command("8", T, X)), opened(&["8"], WaitingHuman)),
            (line(completed(T, X)), gone(&["8"], Idle)),
            (
                answer("8", "0"),
                Seen {
                    error: Some(NOT_OPEN.to_string()),
                    ..Seen::default()
                },
            ),
            // 12: a request from thread C, which the desk's own turns never touch.
            (
                line(command("9", C, "turn-c")),
                opened(&["9"], WaitingHuman),
            ),
            (
                line(turn_note("started", T, Y, "inProgress")),
                Seen {
                    state: Some(SessionState::WaitingHuman),
                    ..Seen::default()
                },
            ),
            // 12: C's turn ends: only its request goes, and the desk's turn runs on.
            (line(completed(C, "turn-c")), gone(&["9"], Thinking)),
            // 10: X's completion again, then a late one of an older turn, twice: Y and
            // its request stay, and the repeat changes nothing at all.
            (line(command("10", T, Y)), opened(&["10"], WaitingHuman)),
            (line(completed(T, X)), Seen::default()),
            (
                line(turn_note("completed", T, "turn-old", "completed")),
                Seen {
                    state: Some(SessionState::WaitingHuman),
                    ..Seen::default()
                },
            ),
            (
                line(turn_note("completed", T, "turn-old", "completed")),
                Seen::default(),
            ),
            // 13: a new turn of the desk's own thread retires the old turn's request.
            (
                line(turn_note("started", T, "turn-three", "inProgress")),
                gone(&["10"], Thinking),
            ),
            // 14: a request that names no turn goes with the turn's end.
            (line(elicitation), opened(&["11"], WaitingHuman)),
            (line(completed(T, "turn-three")), gone(&["11"], Idle)),
        ],
    );
}

/// Row 11: a turn that completes before the response that names it stays over.
#[test]
fn a_late_turn_start_response_does_not_restart_an_ended_turn() {
    use SessionState::{Idle, Thinking};
    let turn_start = |id: u8, text: &str| {
        format!(
            r#"{{"id":{id},"method":"turn/start","params":{{"threadId":"{T}","input":[{{"type":"text","text":"{text}","text_elements":[]}}],"clientUserMessageId":"user-{}","effort":"low"}}}}"#,
            id - 2
        )
    };
    let mut m = desk();
    walk(
        &mut m,
        vec![
            (
                line(completed(T, X)),
                Seen {
                    state: Some(Idle),
                    ..Seen::default()
                },
            ),
            (
                Step::Prompt("TASK-TWO"),
                Seen {
                    written: vec![turn_start(4, "TASK-TWO")],
                    state: Some(Thinking),
                    ..Seen::default()
                },
            ),
            (
                line(completed(T, Y)),
                Seen {
                    state: Some(Idle),
                    ..Seen::default()
                },
            ),
            (
                line(format!(
                    r#"{{"id":4,"result":{{"turn":{{"id":"{Y}","items":[],"status":"inProgress","error":null}}}}}}"#
                )),
                Seen::default(),
            ),
            // Still idle: the next message starts a turn instead of steering one.
            (
                Step::Prompt("TASK-THREE"),
                Seen {
                    written: vec![turn_start(5, "TASK-THREE")],
                    state: Some(Thinking),
                    ..Seen::default()
                },
            ),
        ],
    );
}

/// Row 15: a numeric and a string id are two requests, each answered with its own id.
#[test]
fn a_number_and_a_string_id_are_two_requests() {
    use SessionState::WaitingHuman;
    let mut m = desk();
    walk(
        &mut m,
        vec![
            (line(command("1", T, X)), opened(&["1"], WaitingHuman)),
            (
                line(command(r#""1""#, T, X)),
                opened(&["s:1"], WaitingHuman),
            ),
            (
                answer("s:1", "2"),
                written(&[decision(r#""1""#, "decline")], &["s:1"], WaitingHuman),
            ),
            (
                line(command(r#""1""#, T, X)),
                opened(&["s:1"], WaitingHuman),
            ),
            (line(resolved_note(r#""1""#)), gone(&["s:1"], WaitingHuman)),
            (
                answer("1", "2"),
                written(&[decision("1", "decline")], &["1"], SessionState::Thinking),
            ),
        ],
    );
}

/// Rows 16 to 18: Codex's thread status adds a card only when no request
/// explains it, only after a later line, and only for the desk's own thread.
#[test]
fn the_thread_status_shows_only_what_no_request_explains() {
    use SessionState::{Thinking, WaitingHuman};
    let flags = |flags: &str| format!(r#"{{"type":"active","activeFlags":[{flags}]}}"#);
    let mut m = desk();
    walk(
        &mut m,
        vec![
            // 16
            (
                line(status(T, &flags(r#""waitingOnUserInput""#))),
                Seen::default(),
            ),
            (
                line(questions("12", T, X, Some(false))),
                opened(&["12 background"], Thinking),
            ),
            (line(delta()), Seen::default()),
            (line(status(T, &flags(""))), Seen::default()),
            // 17
            (
                line(status(T, &flags(r#""waitingOnApproval""#))),
                Seen::default(),
            ),
            (
                line(delta()),
                opened(&["status:approval no choices"], WaitingHuman),
            ),
            (line(status(T, &flags(""))), Seen::default()),
            (line(delta()), gone(&["status:approval"], Thinking)),
            // 18
            (
                line(status(C, &flags(r#""waitingOnApproval""#))),
                Seen::default(),
            ),
            (line(delta()), Seen::default()),
            // The card itself takes no answer.
            (
                line(status(T, &flags(r#""waitingOnApproval""#))),
                Seen::default(),
            ),
            (
                line(delta()),
                opened(&["status:approval no choices"], WaitingHuman),
            ),
            (
                answer("status:approval", "0"),
                Seen {
                    error: Some(NOT_OPEN.to_string()),
                    ..Seen::default()
                },
            ),
        ],
    );
}

/// Rows 19 and 20: a resumed thread brings no answer button back, and nothing
/// of its stored turns still runs.
#[test]
fn a_resume_brings_back_no_request() {
    let cases = [
        (r#"{"type":"idle"}"#, Seen::default(), SessionState::Idle),
        (
            r#"{"type":"active","activeFlags":["waitingOnApproval"]}"#,
            opened(&["status:approval no choices"], SessionState::WaitingHuman),
            SessionState::WaitingHuman,
        ),
    ];
    for (status, card, state) in cases {
        let mut m = Mapper::new("/w", None, None, Some(T));
        m.begin("");
        m.line(r#"{"id":1,"result":{}}"#);
        let resumed = format!(
            r#"{{"id":2,"result":{{"thread":{{"id":"{T}","status":{status},"turns":[{{"id":"{X}","status":"interrupted","items":[{{"type":"commandExecution","id":"item-c","command":"tool-x --flag","cwd":"/w","status":"inProgress","commandActions":[],"aggregatedOutput":null,"exitCode":null}}]}}]}}}}}}"#
        );
        let seen = run(&mut m, &line(resumed));
        assert_eq!(
            seen,
            Seen {
                tools: vec![
                    ("item-c".to_string(), ToolStatus::Running),
                    ("item-c".to_string(), ToolStatus::Failed),
                ],
                state: Some(state),
                ..card
            },
            "{status}"
        );
    }
}

/// Row 21: a tool does not outlive its turn, and the next turn thinks until a
/// tool of its own runs.
#[test]
fn a_tool_does_not_outlive_its_turn() {
    use SessionState::{Idle, Running, Thinking};
    let mut m = desk();
    walk(
        &mut m,
        vec![
            (
                line(exec_started("inProgress")),
                Seen {
                    tools: vec![("item-c".to_string(), ToolStatus::Running)],
                    state: Some(SessionState::Running),
                    ..Seen::default()
                },
            ),
            (
                line(command("13", T, X)),
                Seen {
                    opened: vec!["13".to_string()],
                    tools: vec![("item-c".to_string(), ToolStatus::Pending)],
                    state: Some(SessionState::WaitingHuman),
                    ..Seen::default()
                },
            ),
            (
                answer("13", "0"),
                Seen {
                    written: vec![decision("13", "accept")],
                    resolved: vec!["13".to_string()],
                    tools: vec![("item-c".to_string(), ToolStatus::Running)],
                    state: Some(Running),
                    ..Seen::default()
                },
            ),
            (
                line(completed(T, X)),
                Seen {
                    tools: vec![("item-c".to_string(), ToolStatus::Failed)],
                    state: Some(Idle),
                    ..Seen::default()
                },
            ),
            // The next turn thinks until a tool of its own runs.
            (
                line(turn_note("started", T, Y, "inProgress")),
                Seen {
                    state: Some(Thinking),
                    ..Seen::default()
                },
            ),
        ],
    );
}

/// Row 22: lines with fields, item kinds, statuses and flags the building does
/// not know, and a turn notice with no thread, each act as their known part says.
#[test]
fn unknown_parts_of_a_line_never_drop_it() {
    use SessionState::{Idle, Thinking};
    let mut m = desk();
    walk(
        &mut m,
        vec![
            (
                line(format!(
                    r#"{{"method":"turn/started","params":{{"threadId":"{T}","futureField":1,"turn":{{"id":"{Y}","rootTurnId":"{Y}","items":[],"status":"inProgress","error":null,"futureField":true}}}}}}"#
                )),
                Seen {
                    state: Some(Thinking),
                    ..Seen::default()
                },
            ),
            (
                line(format!(
                    r#"{{"method":"item/started","params":{{"threadId":"{T}","turnId":"{Y}","item":{{"type":"futureItem","id":"item-f"}}}}}}"#
                )),
                Seen::default(),
            ),
            (
                line(exec_started("futureStatus")),
                Seen {
                    tools: vec![("item-c".to_string(), ToolStatus::Running)],
                    state: Some(SessionState::Running),
                    ..Seen::default()
                },
            ),
            (
                line(status(T, r#"{"type":"futureStatus","futureField":1}"#)),
                Seen::default(),
            ),
            (
                line(status(
                    T,
                    r#"{"type":"active","activeFlags":["futureFlag"]}"#,
                )),
                Seen::default(),
            ),
            (line(delta()), Seen::default()),
            (
                line(format!(
                    r#"{{"id":14,"method":"item/commandExecution/requestApproval","params":{{"threadId":"{T}","turnId":"{Y}","itemId":"item-c","startedAtMs":1,"command":"tool-x --flag","cwd":"/w","futureField":[1]}}}}"#
                )),
                Seen {
                    opened: vec!["14".to_string()],
                    tools: vec![("item-c".to_string(), ToolStatus::Pending)],
                    state: Some(SessionState::WaitingHuman),
                    ..Seen::default()
                },
            ),
            (
                line(format!(
                    r#"{{"method":"turn/completed","params":{{"turn":{{"id":"{Y}","items":[],"status":"futureStatus","error":null}}}}}}"#
                )),
                Seen {
                    resolved: vec!["14".to_string()],
                    tools: vec![("item-c".to_string(), ToolStatus::Failed)],
                    state: Some(Idle),
                    ..Seen::default()
                },
            ),
        ],
    );
}
