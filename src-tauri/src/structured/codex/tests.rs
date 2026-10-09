//! The mapper fed lines recorded from `codex app-server` 0.161.0 (trimmed to
//! the fields it reads; extra fields are ignored either way), and one live
//! run against the real binary, ignored by default.

use super::mapper::{Mapper, Out};
use crate::model::SessionState;
use crate::structured::{Approval, Choice, Entry, Event, ToolStatus};

const THREAD: &str = "01a11da6-3c44-7b23-8172-0b098dc14e5f";
const TURN: &str = "01a11da6-3c8e-7f03-856c-1cae6ad19b29";

fn sends(out: &[Out]) -> Vec<&str> {
    out.iter()
        .filter_map(|o| match o {
            Out::Send(s) => Some(s.as_str()),
            Out::Event(_) => None,
        })
        .collect()
}

fn events(out: Vec<Out>) -> Vec<Event> {
    out.into_iter()
        .filter_map(|o| match o {
            Out::Event(e) => Some(e),
            Out::Send(_) => None,
        })
        .collect()
}

fn feed(m: &mut Mapper, lines: &[&str]) -> Vec<Out> {
    lines.iter().flat_map(|l| m.line(l)).collect()
}

/// A mapper past the handshake, with `task` running as turn `TURN`.
fn running(task: &str) -> Mapper {
    let mut m = Mapper::new("/w", None, Some("low"), None);
    m.begin(task);
    feed(
        &mut m,
        &[
            r#"{"id":1,"result":{"userAgent":"x","codexHome":"/h","platformFamily":"unix","platformOs":"macos"}}"#,
            &format!(
                r#"{{"id":2,"result":{{"thread":{{"id":"{THREAD}","sessionId":"{THREAD}"}},"model":"m"}}}}"#
            ),
            &format!(
                r#"{{"id":3,"result":{{"turn":{{"id":"{TURN}","items":[],"status":"inProgress","error":null}}}}}}"#
            ),
        ],
    );
    m
}

#[test]
fn handshake_starts_a_thread_and_the_first_turn() {
    let mut m = Mapper::new("/w", Some("gpt-5"), Some("low"), None);
    let out = m.begin("Reply with the word ok");
    assert_eq!(
        sends(&out),
        [
            r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"office_building","title":"Office Building","version":"0.1.0"},"capabilities":{"experimentalApi":false,"requestAttestation":false}}}"#
        ]
    );
    assert_eq!(
        events(out),
        [
            Event::Entry(Entry::User {
                id: "user-1".into(),
                text: "Reply with the word ok".into()
            }),
            Event::State(SessionState::Thinking),
        ]
    );

    let out = m.line(r#"{"id":1,"result":{"userAgent":"x","codexHome":"/h","platformFamily":"unix","platformOs":"macos"}}"#);
    assert_eq!(
        sends(&out),
        [
            r#"{"method":"initialized"}"#,
            r#"{"id":2,"method":"thread/start","params":{"cwd":"/w","model":"gpt-5"}}"#
        ]
    );

    let out = m.line(&format!(
        r#"{{"id":2,"result":{{"thread":{{"id":"{THREAD}","sessionId":"{THREAD}","path":"/h/sessions/rollout-{THREAD}.jsonl"}},"model":"gpt-5"}}}}"#
    ));
    assert_eq!(
        sends(&out),
        [format!(
            r#"{{"id":3,"method":"turn/start","params":{{"threadId":"{THREAD}","input":[{{"type":"text","text":"Reply with the word ok","text_elements":[]}}],"clientUserMessageId":"user-1","effort":"low"}}}}"#
        )]
    );
    assert_eq!(
        events(out),
        [
            Event::SessionId(THREAD.into()),
            Event::State(SessionState::Thinking)
        ]
    );
}

#[test]
fn no_task_waits_for_the_human() {
    let mut m = Mapper::new("/w", None, None, None);
    let out = m.begin("");
    assert_eq!(sends(&out).len(), 1);
    assert!(events(out).is_empty());
    m.line(r#"{"id":1,"result":{}}"#);
    let out = m.line(&format!(
        r#"{{"id":2,"result":{{"thread":{{"id":"{THREAD}"}}}}}}"#
    ));
    assert!(sends(&out).is_empty());
    assert_eq!(
        events(out),
        [
            Event::SessionId(THREAD.into()),
            Event::State(SessionState::Idle)
        ]
    );
}

#[test]
fn streamed_text_grows_one_agent_entry() {
    let mut m = running("hi");
    let item = "msg_0d3c";
    let out = feed(
        &mut m,
        &[
            &format!(
                r#"{{"method":"item/started","params":{{"item":{{"type":"userMessage","id":"01a1","clientId":"user-1","content":[{{"type":"text","text":"hi","text_elements":[]}}]}},"threadId":"{THREAD}","turnId":"{TURN}"}}}}"#
            ),
            &format!(
                r#"{{"method":"item/started","params":{{"item":{{"type":"agentMessage","id":"{item}","text":"","phase":"commentary"}},"threadId":"{THREAD}","turnId":"{TURN}"}}}}"#
            ),
            &format!(
                r#"{{"method":"item/agentMessage/delta","params":{{"threadId":"{THREAD}","turnId":"{TURN}","itemId":"{item}","delta":"I"}}}}"#
            ),
            &format!(
                r#"{{"method":"item/agentMessage/delta","params":{{"threadId":"{THREAD}","turnId":"{TURN}","itemId":"{item}","delta":"’ll run"}}}}"#
            ),
            &format!(
                r#"{{"method":"item/completed","params":{{"item":{{"type":"agentMessage","id":"{item}","text":"I’ll run it.","phase":"commentary"}},"threadId":"{THREAD}","turnId":"{TURN}"}}}}"#
            ),
        ],
    );
    let agent = |text: &str| {
        Event::Entry(Entry::Agent {
            id: item.into(),
            text: text.into(),
        })
    };
    assert_eq!(
        events(out),
        [
            Event::Entry(Entry::User {
                id: "user-1".into(),
                text: "hi".into()
            }),
            agent("I"),
            agent("I’ll run"),
            agent("I’ll run it."),
        ]
    );
}

#[test]
fn reasoning_summary_becomes_a_thought() {
    let mut m = running("hi");
    let out = feed(
        &mut m,
        &[
            r#"{"method":"item/reasoning/summaryPartAdded","params":{"itemId":"rs_1","summaryIndex":0}}"#,
            r#"{"method":"item/reasoning/summaryTextDelta","params":{"itemId":"rs_1","delta":"Looking","summaryIndex":0}}"#,
            r#"{"method":"item/reasoning/summaryTextDelta","params":{"itemId":"rs_1","delta":"Then","summaryIndex":1}}"#,
            r#"{"method":"item/completed","params":{"item":{"type":"reasoning","id":"rs_1","summary":["Looking around","Then"],"content":[]}}}"#,
        ],
    );
    let thought = |text: &str| {
        Event::Entry(Entry::Thought {
            id: "rs_1".into(),
            text: text.into(),
        })
    };
    assert_eq!(
        events(out),
        [
            thought("Looking"),
            thought("Looking\n\nThen"),
            thought("Looking around\n\nThen")
        ]
    );
}

const EXEC_STARTED: &str = r#"{"method":"item/started","params":{"item":{"type":"commandExecution","id":"exec-c7c3","command":"/bin/zsh -lc 'touch probe_ok.txt'","cwd":"/w","processId":null,"source":"agent","status":"inProgress","commandActions":[{"type":"unknown","command":"touch probe_ok.txt"}],"aggregatedOutput":null,"exitCode":null,"durationMs":null}}}"#;
const EXEC_APPROVAL: &str = r#"{"method":"item/commandExecution/requestApproval","id":0,"params":{"kind":"command","threadId":"t","turnId":"u","itemId":"exec-c7c3","startedAtMs":1,"environmentId":"local","command":"/bin/zsh -lc 'touch probe_ok.txt'","cwd":"/w","commandActions":[{"type":"unknown","command":"touch probe_ok.txt"}],"proposedExecpolicyAmendment":["touch","probe_ok.txt"],"availableDecisions":["accept",{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["touch","probe_ok.txt"]}},"cancel"]}}"#;

fn exec(status: ToolStatus, detail: &str) -> Event {
    Event::Entry(Entry::Tool {
        id: "exec-c7c3".into(),
        title: "touch probe_ok.txt".into(),
        status,
        detail: detail.into(),
    })
}

#[test]
fn command_approval_offers_codexs_decisions_and_answers_them() {
    let mut m = running("touch it");
    let out = feed(&mut m, &[EXEC_STARTED, EXEC_APPROVAL]);
    assert_eq!(
        events(out),
        [
            exec(ToolStatus::Running, ""),
            Event::Approval(Approval {
                id: "0".into(),
                title: "Run this command?".into(),
                detail: "touch probe_ok.txt".into(),
                fields: Vec::new(),
                choices: vec![
                    Choice {
                        id: "0".into(),
                        label: "Allow once".into(),
                        allow: true
                    },
                    Choice {
                        id: "1".into(),
                        label: "Always allow `touch probe_ok.txt`".into(),
                        allow: true
                    },
                    Choice {
                        id: "2".into(),
                        label: "Decline and stop the turn".into(),
                        allow: false
                    },
                ],
            }),
            exec(ToolStatus::Pending, ""),
            Event::State(SessionState::WaitingHuman),
        ]
    );

    assert!(
        m.answer("0", "9", &crate::structured::Values::new())
            .is_err()
    );
    let out = m
        .answer("0", "1", &crate::structured::Values::new())
        .unwrap();
    assert_eq!(
        sends(&out),
        [
            r#"{"id":0,"result":{"decision":{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["touch","probe_ok.txt"]}}}}"#
        ]
    );
    assert_eq!(
        events(out),
        [
            Event::Resolved {
                approval_id: "0".into()
            },
            exec(ToolStatus::Running, ""),
            Event::State(SessionState::Thinking),
        ]
    );
    // Codex confirms what we already settled: nothing more to report.
    assert!(
        m.line(r#"{"method":"serverRequest/resolved","params":{"threadId":"t","requestId":0}}"#)
            .is_empty()
    );
    assert!(
        m.answer("0", "0", &crate::structured::Values::new())
            .is_err()
    );

    let out = m.line(r#"{"method":"item/completed","params":{"item":{"type":"commandExecution","id":"exec-c7c3","command":"/bin/zsh -lc 'touch probe_ok.txt'","status":"failed","commandActions":[{"type":"unknown","command":"touch probe_ok.txt"}],"aggregatedOutput":"touch: denied","exitCode":1}}}"#);
    assert_eq!(
        events(out),
        [exec(ToolStatus::Failed, "touch: denied\nExit code 1")]
    );
}

#[test]
fn command_approval_without_a_decision_list_offers_the_defaults() {
    let mut m = running("x");
    let out = m.line(r#"{"method":"item/commandExecution/requestApproval","id":"a","params":{"itemId":"i","command":"ls","cwd":"/elsewhere","reason":"Needs a look"}}"#);
    let Some(Event::Approval(a)) = events(out).into_iter().next() else {
        panic!("no approval");
    };
    assert_eq!(a.detail, "ls\nin /elsewhere\nNeeds a look");
    let labels: Vec<(&str, bool)> = a
        .choices
        .iter()
        .map(|c| (c.label.as_str(), c.allow))
        .collect();
    assert_eq!(
        labels,
        [
            ("Allow once", true),
            ("Allow for this session", true),
            ("Decline", false),
            ("Decline and stop the turn", false)
        ]
    );
    let out = m
        .answer("a", "2", &crate::structured::Values::new())
        .unwrap();
    assert_eq!(
        sends(&out),
        [r#"{"id":"a","result":{"decision":"decline"}}"#]
    );
}

#[test]
fn file_change_approval_shows_the_diff() {
    let mut m = running("edit");
    let out = feed(
        &mut m,
        &[
            r#"{"method":"item/started","params":{"item":{"type":"fileChange","id":"call_1","changes":[{"path":"/w/a.txt","kind":{"type":"add"},"diff":"hello\n"}],"status":"inProgress"}}}"#,
            r#"{"method":"item/fileChange/requestApproval","id":5,"params":{"threadId":"t","turnId":"u","itemId":"call_1","startedAtMs":1,"reason":null,"grantRoot":null}}"#,
        ],
    );
    let ev = events(out);
    assert_eq!(
        ev[0],
        Event::Entry(Entry::Tool {
            id: "call_1".into(),
            title: "Add /w/a.txt".into(),
            status: ToolStatus::Running,
            detail: "/w/a.txt\nhello\n".into()
        })
    );
    let Event::Approval(a) = &ev[1] else {
        panic!("no approval");
    };
    assert_eq!(a.title, "Apply these file changes?");
    assert_eq!(a.detail, "/w/a.txt\nhello\n");
    assert_eq!(a.choices.len(), 4);
    let out = m
        .answer("5", "1", &crate::structured::Values::new())
        .unwrap();
    assert_eq!(
        sends(&out),
        [r#"{"id":5,"result":{"decision":"acceptForSession"}}"#]
    );
}

#[test]
fn a_withdrawn_approval_is_resolved() {
    let mut m = running("x");
    feed(&mut m, &[EXEC_STARTED, EXEC_APPROVAL]);
    let out =
        m.line(r#"{"method":"serverRequest/resolved","params":{"threadId":"t","requestId":0}}"#);
    assert_eq!(
        events(out),
        [
            Event::Resolved {
                approval_id: "0".into()
            },
            Event::State(SessionState::Thinking)
        ]
    );
}

#[test]
fn permissions_grant_echoes_what_was_asked() {
    let mut m = running("x");
    let out = m.line(r#"{"method":"item/permissions/requestApproval","id":7,"params":{"threadId":"t","turnId":"u","itemId":"p","environmentId":null,"startedAtMs":1,"cwd":"/w","reason":"Fetch docs","permissions":{"network":{"enabled":true},"fileSystem":null}}}"#);
    let Some(Event::Approval(a)) = events(out).into_iter().next() else {
        panic!("no approval");
    };
    assert_eq!(a.detail, "Fetch docs\nNetwork access");
    let out = m
        .answer("7", "1", &crate::structured::Values::new())
        .unwrap();
    assert_eq!(
        sends(&out),
        [r#"{"id":7,"result":{"permissions":{"network":{"enabled":true}},"scope":"session"}}"#]
    );
}

fn values(pairs: &[(&str, &str)]) -> crate::structured::Values {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn a_form_elicitation_is_filled_in_and_sent_typed() {
    use crate::structured::FieldKind;
    let mut m = running("x");
    let out = m.line(r#"{"method":"mcpServer/elicitation/request","id":8,"params":{"threadId":"t","turnId":null,"serverName":"brain","mode":"form","_meta":null,"message":"Pick a vault","requestedSchema":{"type":"object","properties":{"vault":{"type":"string","enum":["work","home"]},"limit":{"type":"integer","title":"How many"},"dry":{"type":"boolean"}},"required":["vault"]}}}"#);
    let Some(Event::Approval(a)) = events(out).into_iter().next() else {
        panic!("no approval");
    };
    assert_eq!(
        (a.title.as_str(), a.detail.as_str()),
        ("brain asks for your input", "Pick a vault")
    );
    let fields: Vec<(&str, &FieldKind, bool)> = a
        .fields
        .iter()
        .map(|f| (f.id.as_str(), &f.kind, f.required))
        .collect();
    assert_eq!(
        fields,
        [
            ("dry", &FieldKind::Boolean, false),
            ("limit", &FieldKind::Number, false),
            (
                "vault",
                &FieldKind::Pick {
                    options: vec!["work".into(), "home".into()],
                    other: false
                },
                true
            ),
        ]
    );
    let labels: Vec<&str> = a.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Accept", "Decline", "Cancel"]);
    // A required field left empty is refused, and nothing is sent.
    assert!(m.answer("8", "0", &values(&[("limit", "3")])).is_err());
    assert!(
        m.answer("8", "0", &values(&[("vault", "work"), ("limit", "three")]))
            .is_err()
    );
    let out = m
        .answer(
            "8",
            "0",
            &values(&[("vault", "work"), ("limit", "3"), ("dry", "true")]),
        )
        .unwrap();
    assert_eq!(
        sends(&out),
        [
            r#"{"id":8,"result":{"action":"accept","content":{"dry":true,"limit":3,"vault":"work"},"_meta":null}}"#
        ]
    );
}

#[test]
fn questions_become_fields_answered_together() {
    use crate::structured::FieldKind;
    let mut m = running("x");
    let out = m.line(r#"{"method":"item/tool/requestUserInput","id":9,"params":{"threadId":"t","turnId":"u","itemId":"q","isBlocking":true,"autoResolutionMs":null,"questions":[{"id":"mode","header":"Mode","question":"Which?","isOther":true,"isSecret":false,"options":[{"label":"Fast","description":""},{"label":"Careful","description":"Slower"}]},{"id":"name","header":"","question":"Name?","options":null},{"id":"key","header":"API key","question":"Key?","isSecret":true,"options":null}]}}"#);
    let Some(Event::Approval(a)) = events(out).into_iter().next() else {
        panic!("no approval");
    };
    assert_eq!(
        (a.title.as_str(), a.detail.as_str()),
        ("Codex has some questions", "Careful: Slower")
    );
    let kinds: Vec<(&str, &str, &FieldKind)> = a
        .fields
        .iter()
        .map(|f| (f.id.as_str(), f.label.as_str(), &f.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            (
                "mode",
                "Mode",
                &FieldKind::Pick {
                    options: vec!["Fast".into(), "Careful".into()],
                    other: true
                }
            ),
            ("name", "Name?", &FieldKind::Text),
            ("key", "API key", &FieldKind::Secret),
        ]
    );
    assert_eq!(a.choices.len(), 1);
    let out = m
        .answer(
            "9",
            "0",
            &values(&[("mode", "Something else"), ("name", "Ada"), ("key", "k")]),
        )
        .unwrap();
    assert_eq!(
        sends(&out),
        [
            r#"{"id":9,"result":{"answers":{"key":{"answers":["k"]},"mode":{"answers":["Something else"]},"name":{"answers":["Ada"]}}}}"#
        ]
    );
}

#[test]
fn unknown_and_unreadable_requests_get_an_error() {
    let mut m = running("x");
    let out = m.line(r#"{"method":"item/tool/call","id":11,"params":{"tool":"x"}}"#);
    assert_eq!(
        sends(&out),
        [
            r#"{"id":11,"error":{"code":-32601,"message":"Office Building does not handle item/tool/call"}}"#
        ]
    );
    let out =
        m.line(r#"{"method":"item/tool/requestUserInput","id":12,"params":{"questions":[]}}"#);
    assert_eq!(
        sends(&out),
        [
            r#"{"id":12,"error":{"code":-32601,"message":"Office Building cannot show this request"}}"#
        ]
    );
    let out = m.line(r#"{"method":"item/fileChange/requestApproval","id":13,"params":{}}"#);
    assert!(sends(&out)[0].starts_with(r#"{"id":13,"error":{"code":-32602"#));
}

#[test]
fn prompt_mid_turn_steers_and_cancel_interrupts() {
    let mut m = running("x");
    let out = m.prompt("also this");
    assert_eq!(
        sends(&out),
        [format!(
            r#"{{"id":4,"method":"turn/steer","params":{{"threadId":"{THREAD}","input":[{{"type":"text","text":"also this","text_elements":[]}}],"clientUserMessageId":"user-2","expectedTurnId":"{TURN}"}}}}"#
        )]
    );
    let out = m.cancel();
    assert_eq!(
        sends(&out),
        [format!(
            r#"{{"id":5,"method":"turn/interrupt","params":{{"threadId":"{THREAD}","turnId":"{TURN}"}}}}"#
        )]
    );
    let out = m.line(&format!(r#"{{"method":"turn/completed","params":{{"threadId":"{THREAD}","turn":{{"id":"{TURN}","items":[],"status":"interrupted","error":null}}}}}}"#));
    assert_eq!(
        events(out),
        [
            Event::Entry(Entry::Notice {
                id: format!("stopped-{TURN}"),
                text: "Stopped.".into()
            }),
            Event::State(SessionState::Idle)
        ]
    );
    assert!(m.cancel().is_empty());
    let out = m.prompt("next");
    assert!(sends(&out)[0].contains(r#""method":"turn/start""#));
}

#[test]
fn a_steer_that_missed_its_turn_starts_a_new_one() {
    let mut m = running("x");
    m.prompt("late");
    m.line(&format!(r#"{{"method":"turn/completed","params":{{"turn":{{"id":"{TURN}","status":"completed","error":null}}}}}}"#));
    let out = m.line(r#"{"id":4,"error":{"code":-32600,"message":"no active turn"}}"#);
    let sent = sends(&out);
    assert!(sent[0].contains(r#""method":"turn/start""#) && sent[0].contains("late"));
}

#[test]
fn errors_become_notices_and_a_failed_turn_is_an_error() {
    let mut m = running("x");
    let out = feed(
        &mut m,
        &[
            r#"{"method":"error","params":{"error":{"message":"Reconnecting","codexErrorInfo":null,"additionalDetails":null},"willRetry":true,"threadId":"t","turnId":"u"}}"#,
            r#"{"method":"error","params":{"error":{"message":"Usage limit reached","codexErrorInfo":"usageLimitExceeded","additionalDetails":"Try again at 5pm"},"willRetry":false,"threadId":"t","turnId":"u"}}"#,
            &format!(
                r#"{{"method":"turn/completed","params":{{"turn":{{"id":"{TURN}","status":"failed","error":{{"message":"Usage limit reached","codexErrorInfo":null,"additionalDetails":null}}}}}}}}"#
            ),
        ],
    );
    assert_eq!(
        events(out),
        [
            Event::Entry(Entry::Notice {
                id: "notice-1".into(),
                text: "Reconnecting (Codex is retrying)".into()
            }),
            Event::Entry(Entry::Notice {
                id: "notice-2".into(),
                text: "Usage limit reached\nTry again at 5pm".into()
            }),
            Event::State(SessionState::Error),
        ]
    );
}

#[test]
fn a_refused_thread_is_an_error_and_noise_is_skipped() {
    let mut m = Mapper::new("/w", None, None, None);
    m.begin("");
    assert!(m.line("Welcome to zsh").is_empty());
    m.line(r#"{"id":1,"result":{}}"#);
    let out = m.line(r#"{"id":2,"error":{"code":-32603,"message":"not logged in"}}"#);
    assert_eq!(
        events(out),
        [
            Event::Entry(Entry::Notice {
                id: "notice-1".into(),
                text: "Codex did not start a conversation: not logged in".into()
            }),
            Event::State(SessionState::Error)
        ]
    );
}

#[test]
fn completed_turn_goes_idle_and_plan_updates_show() {
    let mut m = running("x");
    let out = feed(
        &mut m,
        &[
            &format!(
                r#"{{"method":"turn/plan/updated","params":{{"threadId":"t","turnId":"{TURN}","explanation":null,"plan":[{{"step":"Read","status":"completed"}},{{"step":"Write","status":"inProgress"}}]}}}}"#
            ),
            &format!(
                r#"{{"method":"turn/completed","params":{{"threadId":"t","turn":{{"id":"{TURN}","items":[],"status":"completed","error":null}}}}}}"#
            ),
        ],
    );
    assert_eq!(
        events(out),
        [
            Event::Entry(Entry::Notice {
                id: format!("plan-{TURN}"),
                text: "- [x] Read\n- [~] Write".into()
            }),
            Event::State(SessionState::Idle)
        ]
    );
}

/// Run `task` in a fresh folder with the real Codex until the turn ends,
/// handing every event to `seen` (which may answer approvals), then stop it
/// and check the exit was clean. Returns the thread id and the folder.
fn live(
    name: &str,
    launch: &crate::setup::Launch,
    task: &str,
    mut seen: impl FnMut(&dyn super::Driver, &Event),
) -> (String, std::path::PathBuf) {
    use crate::model::ToolKind;
    use crate::structured::{Sink, Spawn};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    let dir = std::env::temp_dir().join(format!("codex-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: Sink = Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    let spawn = Spawn {
        tool: ToolKind::Codex,
        cwd: &dir,
        model: None,
        effort: Some("low"),
        launch,
        task,
        resume: None,
    };
    let driver = super::start(&spawn, sink).unwrap();
    let mut session = None;
    loop {
        let e = rx
            .recv_timeout(Duration::from_secs(120))
            .expect("Codex went quiet");
        println!("{e:?}");
        seen(driver.as_ref(), &e);
        match e {
            Event::SessionId(id) => session = Some(id),
            Event::State(SessionState::Idle) if session.is_some() => break,
            Event::State(SessionState::Error) | Event::Exited { .. } => panic!("{e:?}"),
            _ => {}
        }
    }
    driver.stop();
    loop {
        if let Event::Exited { message, .. } = rx.recv_timeout(Duration::from_secs(10)).unwrap() {
            assert_eq!(message, None);
            break;
        }
    }
    (session.unwrap(), dir)
}

/// Whether a file under `dir` (searched to any depth) has `needle` in its name.
fn has_file(dir: &std::path::Path, needle: &str) -> bool {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            let path = e.path();
            if path.is_dir() {
                has_file(&path, needle)
            } else {
                e.file_name().to_string_lossy().contains(needle)
            }
        })
}

/// A Codex home of the test's own, with only the real login linked in, so
/// live runs never land in the human's session history (where the building
/// would give each temporary folder a floor). Returns the home and its launch.
fn own_home(name: &str) -> (std::path::PathBuf, crate::setup::Launch) {
    let home = std::env::temp_dir().join(format!("codex-home-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let auth = home.join("auth.json");
    if !auth.exists() {
        std::os::unix::fs::symlink(dirs::home_dir().unwrap().join(".codex/auth.json"), &auth)
            .unwrap();
    }
    let launch = crate::setup::Launch {
        env: vec![("CODEX_HOME", home.to_string_lossy().into_owned())],
        opts: Vec::new(),
    };
    (home, launch)
}

/// A real turn: `cargo test --lib live_ -- --ignored --nocapture`.
#[test]
#[ignore = "runs the real Codex and spends a little of the user's plan"]
fn live_turn() {
    let mut reply = String::new();
    let (home, launch) = own_home("turn");
    let (thread, dir) = live("turn", &launch, "Reply with the word ok", |_, e| {
        if let Event::Entry(Entry::Agent { text, .. }) = e {
            reply.clone_from(text);
        }
    });
    assert!(reply.to_lowercase().contains("ok"));
    // The id reported is the one the rollout file is named after.
    assert!(has_file(&home.join("sessions"), &thread));
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

/// An approval answered through the driver. The launch's environment points
/// Codex at a home of its own (the real login linked in) that asks before
/// anything untrusted runs, with a read-only sandbox.
#[test]
#[ignore = "runs the real Codex and spends a little of the user's plan"]
fn live_approval() {
    let home = std::env::temp_dir().join(format!("codex-home-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::os::unix::fs::symlink(
        dirs::home_dir().unwrap().join(".codex/auth.json"),
        home.join("auth.json"),
    )
    .unwrap();
    std::fs::write(
        home.join("config.toml"),
        "approval_policy = \"on-request\"\nsandbox_mode = \"read-only\"\n",
    )
    .unwrap();
    let launch = crate::setup::Launch {
        env: vec![("CODEX_HOME", home.to_string_lossy().into_owned())],
        opts: Vec::new(),
    };
    let mut asked = false;
    let (thread, dir) = live(
        "approval",
        &launch,
        "Run the shell command `touch approved.txt` in the current folder, then reply done.",
        |driver, e| {
            if let Event::Approval(a) = e {
                asked = true;
                let allow = a.choices.iter().find(|c| c.allow).unwrap();
                driver
                    .answer(&a.id, &allow.id, &crate::structured::Values::new())
                    .unwrap();
            }
        },
    );
    assert!(asked);
    assert!(dir.join("approved.txt").exists());
    assert!(has_file(&home.join("sessions"), &thread));
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

/// Stopping a turn as soon as the agent starts writing.
#[test]
#[ignore = "runs the real Codex and spends a little of the user's plan"]
fn live_cancel() {
    let mut cancelled = false;
    let mut stopped = false;
    let (home, launch) = own_home("cancel");
    let (_, dir) = live(
        "cancel",
        &launch,
        "Write the numbers from 1 to 300, one per line, with no other text.",
        |driver, e| match e {
            Event::Entry(Entry::Agent { .. }) if !cancelled => {
                cancelled = true;
                driver.cancel().unwrap();
            }
            Event::Entry(Entry::Notice { text, .. }) if text == "Stopped." => stopped = true,
            _ => {}
        },
    );
    assert!(cancelled && stopped);
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

/// A thread reopened after its agent stopped: the earlier turn comes back as
/// entries, and the desk waits idle.
#[test]
#[ignore = "runs the real Codex and spends a little of the user's plan"]
fn live_resume() {
    use crate::model::ToolKind;
    use crate::structured::{Sink, Spawn};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    let (home, launch) = own_home("resume");
    let (thread, dir) = live("resume", &launch, "Reply with the word ok", |_, _| {});
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: Sink = Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    let spawn = Spawn {
        tool: ToolKind::Codex,
        cwd: &dir,
        model: None,
        effort: Some("low"),
        launch: &launch,
        task: "",
        resume: Some(&thread),
    };
    let driver = super::start(&spawn, sink).unwrap();
    let (mut asked, mut answered, mut id) = (false, false, None);
    loop {
        let e = rx
            .recv_timeout(Duration::from_secs(120))
            .expect("Codex went quiet");
        println!("{e:?}");
        match e {
            Event::SessionId(s) => id = Some(s),
            Event::Entry(Entry::User { text, .. }) if text.contains("Reply with the word ok") => {
                asked = true;
            }
            Event::Entry(Entry::Agent { text, .. }) if text.to_lowercase().contains("ok") => {
                answered = true;
            }
            Event::State(SessionState::Idle) if id.is_some() => break,
            Event::State(SessionState::Error) | Event::Exited { .. } => panic!("{e:?}"),
            _ => {}
        }
    }
    driver.stop();
    assert_eq!(id.as_deref(), Some(thread.as_str()));
    assert!(asked && answered, "the earlier turn came back");
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}
