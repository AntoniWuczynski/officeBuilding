//! Lines below were recorded from `opencode acp` 1.18.33 and `cursor-agent acp`
//! 2026.10.01 (paths shortened), unless a test says otherwise.

use super::transcript::Transcript;
use super::wire::{self, Inbound, ModelChoices, StopReason};
use super::{ModelCall, model_call};
use crate::model::SessionState;
use crate::structured::{Approval, Choice, Entry, Event, FieldKind, ToolStatus, Values};

/// Feed agent lines into a transcript the way the reader does; returns the
/// events and reply lines.
fn feed(t: &mut Transcript, lines: &[&str]) -> (Vec<Event>, Vec<String>) {
    let mut events = Vec::new();
    let mut replies = Vec::new();
    for line in lines {
        let (ev, reply) = match wire::parse(line).expect("a JSON-RPC line") {
            Inbound::Request { id, method, params } => t.on_request(id, &method, params.as_deref()),
            Inbound::Notification { method, params } => {
                t.on_notification(&method, params.as_deref())
            }
            Inbound::Response { .. } => panic!("not a line from the agent's side"),
        };
        events.extend(ev);
        replies.extend(reply.map(|r| r.expect("a reply line")));
    }
    (events, replies)
}

fn update(json: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"ses_1","update":{json}}}}}"#
    )
}

const PERMISSION: &str = r#"{"jsonrpc":"2.0","id":0,"method":"session/request_permission","params":{"sessionId":"ses_1","toolCall":{"toolCallId":"call_1","title":"echo hi","kind":"execute","status":"pending","locations":[],"rawInput":{"command":"echo hi"}},"options":[{"optionId":"once","kind":"allow_once","name":"Allow once"},{"optionId":"always","kind":"allow_always","name":"Always allow"},{"optionId":"reject","kind":"reject_once","name":"Reject"}]}}"#;

fn tool(status: ToolStatus, title: &str, detail: &str) -> Event {
    Event::Entry(Entry::Tool {
        id: "tool-call_1".to_string(),
        title: title.to_string(),
        status,
        detail: detail.to_string(),
    })
}

#[test]
fn lines_are_sorted_into_responses_requests_and_notifications() {
    assert!(matches!(
        wire::parse(r#"{"jsonrpc":"2.0","id":4,"result":{"stopReason":"end_turn","_meta":{}}}"#),
        Some(Inbound::Response {
            id: 4,
            reply: Ok(Some(_))
        })
    ));
    assert!(matches!(
        wire::parse(r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"Authentication required","data":{"message":"Please run 'agent login' first"}}}"#),
        Some(Inbound::Response { id: 2, reply: Err(e) }) if e.code == wire::AUTH_REQUIRED && e.text() == "Please run 'agent login' first"
    ));
    assert!(matches!(
        wire::parse(PERMISSION),
        Some(Inbound::Request {
            id: wire::Id::Number(0),
            ..
        })
    ));
    assert!(matches!(
        wire::parse(&update(
            r#"{"sessionUpdate":"usage_update","used":1,"size":2}"#
        )),
        Some(Inbound::Notification { .. })
    ));
    assert!(wire::parse("Welcome to zsh").is_none());
}

#[test]
fn a_tool_call_and_its_updates_become_one_tool_entry() {
    let mut t = Transcript::default();
    let (events, replies) = feed(
        &mut t,
        &[
            &update(
                r#"{"sessionUpdate":"tool_call","toolCallId":"call_1","title":"bash","kind":"execute","status":"pending","locations":[{"path":"/ws"}],"rawInput":{"cwd":"/ws"}}"#,
            ),
            &update(
                r#"{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"in_progress","kind":"execute","title":"echo hi","rawInput":{"command":"echo hi","cwd":"/ws"}}"#,
            ),
            &update(
                r#"{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"in_progress","kind":"execute","title":"echo hi","content":[{"type":"content","content":{"type":"text","text":"hi\n"}}]}"#,
            ),
            &update(
                r#"{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"completed","title":"echo hi","content":[{"type":"content","content":{"type":"text","text":"hi\n"}}],"rawOutput":{"output":"hi\n","metadata":{"exit":0}}}"#,
            ),
        ],
    );
    assert!(replies.is_empty());
    assert_eq!(
        events,
        vec![
            tool(ToolStatus::Pending, "bash", ""),
            tool(ToolStatus::Running, "echo hi", ""),
            tool(ToolStatus::Running, "echo hi", "hi\n"),
            tool(ToolStatus::Done, "echo hi", "hi\n"),
        ]
    );
}

#[test]
fn message_chunks_grow_one_entry_per_message() {
    let mut t = Transcript::default();
    let chunk = |kind: &str, id: &str, text: &str| {
        update(&format!(
            r#"{{"sessionUpdate":"{kind}","messageId":"{id}","content":{{"type":"text","text":"{text}"}}}}"#
        ))
    };
    let (events, _) = feed(
        &mut t,
        &[
            &chunk("agent_thought_chunk", "m1", "Think"),
            &chunk("agent_thought_chunk", "m1", "ing"),
            &chunk("agent_message_chunk", "m1", "Do"),
            &chunk("agent_message_chunk", "m1", "ne."),
            &chunk("agent_message_chunk", "m2", "Next"),
            // Cursor sends no message id: chunks still join until something else comes.
            &update(
                r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"A"}}"#,
            ),
            &update(
                r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"B"}}"#,
            ),
        ],
    );
    let texts: Vec<(&str, &str)> = events
        .iter()
        .map(|e| match e {
            Event::Entry(Entry::Agent { id, text } | Entry::Thought { id, text }) => {
                (id.as_str(), text.as_str())
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        texts,
        vec![
            ("thought-1", "Think"),
            ("thought-1", "Thinking"),
            ("agent-2", "Do"),
            ("agent-2", "Done."),
            ("agent-3", "Next"),
            ("agent-4", "A"),
            ("agent-4", "AB"),
        ]
    );
}

#[test]
fn a_permission_request_waits_for_the_human_and_answers_with_the_choice() {
    let mut t = Transcript::default();
    t.user("run echo hi");
    let (events, replies) = feed(&mut t, &[PERMISSION]);
    assert!(
        replies.is_empty(),
        "the agent waits until the human answers"
    );
    let choice = |id: &str, label: &str, allow| Choice {
        id: id.to_string(),
        label: label.to_string(),
        allow,
    };
    assert_eq!(
        events,
        vec![
            tool(ToolStatus::Pending, "echo hi", ""),
            Event::Approval(Approval {
                id: "approval-2".to_string(),
                title: "echo hi".to_string(),
                detail: r#"{"command":"echo hi"}"#.to_string(),
                fields: Vec::new(),
                choices: vec![
                    choice("once", "Allow once", true),
                    choice("always", "Always allow", true),
                    choice("reject", "Reject", false),
                ],
            }),
            Event::State(SessionState::WaitingHuman),
        ]
    );
    assert!(t.answer("approval-2", "never", &Values::new()).is_err());
    let answer = t
        .answer("approval-2", "once", &Values::new())
        .expect("an open approval");
    assert_eq!(
        answer.reply,
        r#"{"jsonrpc":"2.0","id":0,"result":{"outcome":{"outcome":"selected","optionId":"once"}}}"#
    );
    assert_eq!(
        answer.events,
        vec![
            Event::Resolved {
                approval_id: "approval-2".to_string()
            },
            Event::State(SessionState::Thinking),
        ]
    );
    assert!(
        t.answer("approval-2", "once", &Values::new()).is_err(),
        "answered once only"
    );
}

#[test]
fn requests_the_building_does_not_serve_get_an_error_reply() {
    let mut t = Transcript::default();
    let (events, replies) = feed(
        &mut t,
        &[
            r#"{"jsonrpc":"2.0","id":"r7","method":"fs/read_text_file","params":{"sessionId":"ses_1","path":"/a"}}"#,
            r#"{"jsonrpc":"2.0","id":8,"method":"terminal/create","params":{"sessionId":"ses_1","command":"ls"}}"#,
        ],
    );
    assert!(events.is_empty());
    assert_eq!(
        replies,
        vec![
            r#"{"jsonrpc":"2.0","id":"r7","error":{"code":-32601,"message":"Office Building does not provide fs/read_text_file"}}"#,
            r#"{"jsonrpc":"2.0","id":8,"error":{"code":-32601,"message":"Office Building does not provide terminal/create"}}"#,
        ]
    );
}

#[test]
fn cancelling_answers_open_approvals_and_fails_unfinished_tools() {
    let mut t = Transcript::default();
    t.user("go");
    feed(&mut t, &[PERMISSION]);
    let (replies, events) = t.cancel();
    let replies: Vec<String> = replies.into_iter().map(|r| r.expect("a line")).collect();
    assert_eq!(
        replies,
        vec![r#"{"jsonrpc":"2.0","id":0,"result":{"outcome":{"outcome":"cancelled"}}}"#]
    );
    assert_eq!(
        events,
        vec![
            Event::Resolved {
                approval_id: "approval-2".to_string()
            },
            tool(ToolStatus::Failed, "echo hi", ""),
            Event::State(SessionState::Thinking),
        ]
    );
    let ended = t.turn_ended(Ok(StopReason::Cancelled));
    assert_eq!(ended, vec![Event::State(SessionState::Idle)]);
    assert!(!t.turn_running());
}

#[test]
fn a_withdrawn_request_is_resolved_and_closed() {
    let mut t = Transcript::default();
    feed(&mut t, &[PERMISSION]);
    let (events, replies) = feed(
        &mut t,
        &[r#"{"jsonrpc":"2.0","method":"$/cancel_request","params":{"requestId":0}}"#],
    );
    assert_eq!(
        events,
        vec![Event::Resolved {
            approval_id: "approval-1".to_string()
        }]
    );
    assert_eq!(
        replies,
        vec![
            r#"{"jsonrpc":"2.0","id":0,"error":{"code":-32800,"message":"withdrawn by the agent"}}"#
        ]
    );
}

#[test]
fn a_plan_is_one_notice_that_each_update_replaces() {
    let mut t = Transcript::default();
    let plan = |second: &str| {
        update(&format!(
            r#"{{"sessionUpdate":"plan","entries":[{{"content":"Read","priority":"high","status":"completed"}},{{"content":"Write","priority":"medium","status":"{second}"}}]}}"#
        ))
    };
    let (events, _) = feed(&mut t, &[&plan("in_progress"), &plan("pending")]);
    assert_eq!(
        events,
        vec![
            Event::Entry(Entry::Notice {
                id: "plan-1".to_string(),
                text: "Plan:\n- [x] Read\n- [ ] (in progress) Write".to_string(),
            }),
            Event::Entry(Entry::Notice {
                id: "plan-1".to_string(),
                text: "Plan:\n- [x] Read\n- [ ] Write".to_string(),
            }),
        ]
    );
}

#[test]
fn a_diff_shows_only_the_changed_lines() {
    let mut t = Transcript::default();
    let (events, _) = feed(
        &mut t,
        &[&update(
            r#"{"sessionUpdate":"tool_call","toolCallId":"call_1","title":"Edit a.txt","kind":"edit","status":"completed","content":[{"type":"diff","path":"/ws/a.txt","oldText":"one\ntwo\nthree\n","newText":"one\n2\nthree\n"}]}"#,
        )],
    );
    assert_eq!(
        events,
        vec![tool(
            ToolStatus::Done,
            "Edit a.txt",
            "--- /ws/a.txt\n+++ /ws/a.txt\n-two\n+2"
        )]
    );
}

#[test]
fn unusual_stops_and_errors_leave_a_notice() {
    let mut t = Transcript::default();
    t.user("go");
    assert_eq!(
        t.turn_ended(Ok(StopReason::MaxTokens)),
        vec![
            Event::Entry(Entry::Notice {
                id: "notice-2".to_string(),
                text: "The agent stopped: it reached its token limit.".to_string(),
            }),
            Event::State(SessionState::Idle),
        ]
    );
    let Some(Inbound::Response { reply: Err(e), .. }) = wire::parse(
        r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"Authentication required"}}"#,
    ) else {
        panic!("an error response");
    };
    assert!(super::transcript::error_text(&e).starts_with("The agent needs you to sign in."));
}

#[test]
fn the_model_is_found_in_config_options_or_cursors_model_list() {
    let opencode: ModelChoices = serde_json::from_str(r#"{"sessionId":"ses_1","configOptions":[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"opencode/big-pickle","options":[{"value":"opencode/big-pickle","name":"OpenCode Zen/Big Pickle"},{"value":"opencode/exo-free","name":"OpenCode Zen/Exo Free"}]},{"id":"mode","name":"Session Mode","category":"mode","type":"select","currentValue":"build","options":[{"value":"build","name":"build"}]},{"id":"brave","name":"Brave","type":"boolean","currentValue":true}]}"#).expect("opencode's session");
    assert_eq!(
        model_call(&opencode, "opencode/exo-free"),
        Some(ModelCall::ConfigOption {
            config_id: "model".to_string(),
            value: "opencode/exo-free".to_string()
        })
    );
    assert_eq!(
        model_call(&opencode, "build"),
        None,
        "a mode is not a model"
    );

    let cursor: ModelChoices = serde_json::from_str(r#"{"sessionId":"f250","modes":{"currentModeId":"agent","availableModes":[]},"models":{"currentModelId":"default[]","availableModels":[{"modelId":"default[]","name":"Auto"},{"modelId":"gpt-5.5[context=272k,reasoning=medium,fast=false]","name":"gpt-5.5"}]}}"#).expect("Cursor's session");
    assert_eq!(
        model_call(&cursor, "gpt-5.5"),
        Some(ModelCall::SetModel {
            model_id: "gpt-5.5[context=272k,reasoning=medium,fast=false]".to_string()
        })
    );
    assert_eq!(model_call(&cursor, "gpt-9"), None);
    assert_eq!(model_call(&ModelChoices::default(), "gpt-5.5"), None);
}

#[test]
fn requests_are_written_as_the_schema_names_them() {
    let line = wire::request(
        2,
        "session/new",
        wire::NewSessionParams {
            cwd: "/ws",
            mcp_servers: [],
        },
    )
    .expect("a line");
    assert_eq!(
        line,
        r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/ws","mcpServers":[]}}"#
    );
    let line = wire::request(
        4,
        "session/prompt",
        wire::PromptParams {
            session_id: "ses_1",
            prompt: [wire::TextBlock {
                kind: "text",
                text: "hi",
            }],
        },
    )
    .expect("a line");
    assert_eq!(
        line,
        r#"{"jsonrpc":"2.0","id":4,"method":"session/prompt","params":{"sessionId":"ses_1","prompt":[{"type":"text","text":"hi"}]}}"#
    );
}

/// Runs the real agent: a turn that asks to run `echo hi`, approved once.
/// `cargo test live_ -- --ignored --nocapture`.
mod live {
    use super::super::start;
    use crate::model::{SessionState, ToolKind};
    use crate::setup::Launch;
    use crate::structured::{Entry, Event, Spawn, ToolStatus};
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    fn run(tool: ToolKind, model: Option<&str>, env: Vec<(&'static str, String)>) {
        let dir = std::env::temp_dir().join(format!("acp-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let launch = Launch {
            env,
            opts: Vec::new(),
        };
        let (tx, rx) = mpsc::channel();
        let driver = start(
            &Spawn {
                tool,
                cwd: &dir,
                model,
                effort: None,
                launch: &launch,
                task: "Run the shell command `echo hi` with your shell tool, then reply with one word.",
                resume: None,
            },
            Arc::new(move |e| {
                let _ = tx.send(e);
            }),
        )
        .expect("the agent starts");
        let until = Instant::now() + Duration::from_mins(3);
        let (mut session, mut approved, mut tool_done, mut idle) = (false, false, false, false);
        while !idle && Instant::now() < until {
            let Ok(e) = rx.recv_timeout(Duration::from_secs(5)) else {
                continue;
            };
            println!("{e:?}");
            match e {
                Event::SessionId(_) => session = true,
                Event::Approval(a) => {
                    let allow = a.choices.iter().find(|c| c.allow).expect("an allow choice");
                    driver
                        .answer(&a.id, &allow.id, &crate::structured::Values::new())
                        .expect("the answer is sent");
                    approved = true;
                }
                Event::Entry(Entry::Tool {
                    status: ToolStatus::Done,
                    ..
                }) => tool_done = true,
                Event::State(SessionState::Idle) => idle = true,
                Event::Exited { .. } => panic!("the agent exited early"),
                _ => {}
            }
        }
        driver.stop();
        let exited = rx.iter().any(|e| matches!(e, Event::Exited { .. }));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(session, "a session id");
        assert!(idle, "the turn completed");
        assert!(tool_done, "the command ran");
        assert!(exited, "the exit is reported");
        println!("approval asked and answered: {approved}");
    }

    #[test]
    #[ignore = "starts opencode and uses a free model over the network"]
    fn live_opencode_turn_with_an_approval() {
        run(
            ToolKind::Opencode,
            Some("opencode/big-pickle"),
            vec![(
                "OPENCODE_CONFIG_CONTENT",
                r#"{"permission":{"bash":"ask"}}"#.to_string(),
            )],
        );
    }

    #[test]
    #[ignore = "starts Cursor, which must be signed in, and spends its quota"]
    fn live_cursor_turn() {
        run(ToolKind::Cursor, None, Vec::new());
    }
}

#[test]
fn cursor_questions_are_fields_answered_by_option_id() {
    let mut t = Transcript::default();
    let (events, replies) = feed(
        &mut t,
        &[
            r#"{"jsonrpc":"2.0","id":5,"method":"cursor/ask_question","params":{"toolCallId":"c1","title":"Which database?","questions":[{"id":"db","prompt":"Database","options":[{"id":"pg","label":"Postgres"},{"id":"lite","label":"SQLite"}]}]}}"#,
        ],
    );
    assert!(replies.is_empty(), "the agent waits for the human");
    let Some(Event::Approval(a)) = events.first() else {
        panic!("no approval: {events:?}");
    };
    assert_eq!(a.title, "Which database?");
    assert_eq!(
        a.fields[0].kind,
        FieldKind::Pick {
            options: vec!["Postgres".to_string(), "SQLite".to_string()],
            other: false
        }
    );
    assert_eq!(
        events.last(),
        Some(&Event::State(SessionState::WaitingHuman))
    );
    let id = a.id.clone();
    assert!(
        t.answer(&id, "answer", &Values::new()).is_err(),
        "every question needs an answer"
    );
    let values: Values = [("db".to_string(), "SQLite".to_string())].into();
    let answer = t.answer(&id, "answer", &values).expect("answered");
    assert_eq!(
        answer.reply,
        r#"{"jsonrpc":"2.0","id":5,"result":{"outcome":{"outcome":"answered","answers":[{"questionId":"db","selectedOptionIds":["lite"]}]}}}"#
    );
}

#[test]
fn a_cursor_plan_is_shown_and_accepted_or_cancelled_with_the_turn() {
    let mut t = Transcript::default();
    let (events, _) = feed(
        &mut t,
        &[
            r#"{"jsonrpc":"2.0","id":6,"method":"cursor/create_plan","params":{"toolCallId":"c2","name":"Tidy","overview":"Clean up","plan":"1. Lint\n2. Test","todos":[{"id":"a","content":"Lint","status":"pending"},{"id":"b","content":"Test","status":"completed"}]}}"#,
        ],
    );
    let Some(Event::Approval(a)) = events.first() else {
        panic!("no approval: {events:?}");
    };
    assert_eq!(a.title, "Cursor proposes a plan: Tidy");
    assert_eq!(
        a.detail,
        "Clean up\n\n1. Lint\n2. Test\n\n- [ ] Lint\n- [x] Test"
    );
    let labels: Vec<&str> = a.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Accept the plan", "Reject", "Cancel"]);
    // Stopping the turn answers it as cancelled.
    let (replies, _) = t.cancel();
    assert_eq!(
        replies[0].as_deref(),
        Ok(r#"{"jsonrpc":"2.0","id":6,"result":{"outcome":{"outcome":"cancelled"}}}"#)
    );
}

#[test]
fn cursor_notifications_become_notes() {
    let mut t = Transcript::default();
    let (events, replies) = feed(
        &mut t,
        &[
            r#"{"jsonrpc":"2.0","method":"cursor/update_todos","params":{"toolCallId":"c3","todos":[{"id":"a","content":"Write tests","status":"in_progress"}],"merge":true}}"#,
            r#"{"jsonrpc":"2.0","method":"cursor/task","params":{"toolCallId":"c4","description":"Explore the repo","prompt":"x","subagentType":"explore"}}"#,
        ],
    );
    assert!(replies.is_empty());
    let texts: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::Entry(Entry::Notice { text, .. }) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        [
            "To-dos:\n- [ ] (in progress) Write tests",
            "Started a sub-agent: Explore the repo"
        ]
    );
}
