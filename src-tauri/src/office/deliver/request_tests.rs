//! The delivery gate with a Codex request open in an app terminal (spec 5.6, 8.2): the gate
//! reads the reconciled session, so a manager's note is never typed into a Codex terminal
//! while a request is open. Hook files are composed from F19 (Src 161
//! `hooks/src/schema.rs`), not replayed: placeholder ids and commands only.

use super::super::tests::floor_office;
use super::tests::{hold_of, queue, screen, seat, wait_for};
use super::*;
use crate::discovery::codex::turns::Evidence;
use crate::model::{AnswerVia, Project, Question};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

const NOTE: &str = "Take the next TODO item.";

/// Drop a Codex hook file of turn `t1` for `session` and leave it for the next settle.
/// `call` and `cmd` name the tool call, empty for an event that has none.
fn hook(office: &Office, session: &str, name: &str, call: &str, cmd: &str) {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    // A newer event must carry a later time than the one before it.
    std::thread::sleep(Duration::from_millis(15));
    let id = if call.is_empty() {
        String::new()
    } else {
        format!(r#","tool_use_id":"{call}""#)
    };
    let tool = if cmd.is_empty() {
        String::new()
    } else {
        format!(r#","tool_name":"shell","tool_input":{{"command":"{cmd}"}}"#)
    };
    let dir = office.lock().feed.dir().to_path_buf();
    fs::create_dir_all(&dir).unwrap();
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    fs::write(
        dir.join(format!("{name}.{n:06}")),
        format!(r#"{{"session_id":"{session}","turn_id":"t1"{id}{tool}}}"#),
    )
    .unwrap();
}

/// A permission request for call `c1`, as Codex's hooks report it.
fn ask(office: &Office, session: &str) {
    hook(office, session, "PreToolUse", "c1", "tool-x --flag");
    hook(office, session, "PermissionRequest", "", "tool-x --flag");
}

/// A row discovery made for `session` from its rollout.
fn row(session: &str, suffix: &str) -> Question {
    Question {
        id: format!("{session}:{suffix}"),
        project_id: String::new(),
        session_id: session.to_string(),
        prompt: "QUESTION-ONE".to_string(),
        options: Vec::new(),
        allow_other: true,
        answer_via: AnswerVia::Terminal,
        priority: i32::from(suffix == "prose"),
        asked_at: String::new(),
        context: String::new(),
        source_file: None,
        handoff: None,
    }
}

/// A fresh scan, as `rescan` makes one: discovery's view of the desk (`own`, its `rows`, its
/// rollout's evidence and last activity), then the feed collected and the requests settled.
fn rescan(
    office: &Office,
    session: &str,
    own: SessionState,
    rows: Vec<Question>,
    evidence: Option<Evidence>,
    last: Option<String>,
) {
    let mut guard = office.lock();
    let inner = &mut *guard;
    let mut found = std::mem::take(&mut inner.found);
    let s = found.sessions.iter_mut().find(|s| s.id == session).unwrap();
    s.state = own;
    s.pending_question_ids = rows.iter().map(|q| q.id.clone()).collect();
    if let Some(last) = last {
        s.last_activity_at = last;
    }
    let project = s.project_id.clone();
    found.questions.retain(|q| q.session_id != session);
    found.questions.extend(rows.into_iter().map(|q| Question {
        project_id: project.clone(),
        ..q
    }));
    found.codex.clear();
    if let Some(evidence) = evidence {
        found.codex.insert(session.to_string(), evidence);
    }
    inner.settle(&mut found, SystemTime::now());
    inner.found = found;
}

fn typed(office: &Office, terminal: &str) -> bool {
    screen(office, terminal).contains(NOTE)
}

/// Held as waiting on A Human, and nothing reaches the terminal.
fn held(office: &Office, floor: &Project, id: &str, terminal: &str) {
    for _ in 0..3 {
        assert!(!office.deliver_due());
        assert_eq!(hold_of(office, floor, id), Some(Hold::WaitingOnYou));
        std::thread::sleep(Duration::from_millis(30));
    }
    assert!(!typed(office, terminal));
}

fn delivered(office: &Office, floor: &Project, id: &str, terminal: &str) {
    assert_eq!(hold_of(office, floor, id), None);
    assert!(office.deliver_due());
    wait_for("the note typed", || typed(office, terminal));
}

#[test]
fn a_permission_request_holds_the_note_until_its_call_ran_and_the_turn_ended() {
    let (office, dir, floor) = floor_office("deliver-request-hook");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::Codex,
        SessionState::Running,
    );
    ask(&office, &id);
    rescan(&office, &id, SessionState::Running, Vec::new(), None, None);
    queue(&office, &floor, &id, NOTE);
    held(&office, &floor, &id, &terminal);
    // Rescans with nothing new keep it held.
    rescan(&office, &id, SessionState::Running, Vec::new(), None, None);
    held(&office, &floor, &id, &terminal);
    hook(&office, &id, "PostToolUse", "c1", "tool-x --flag");
    hook(&office, &id, "Stop", "", "");
    rescan(&office, &id, SessionState::Idle, Vec::new(), None, None);
    delivered(&office, &floor, &id, &terminal);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_open_input_request_holds_the_note_until_it_is_answered() {
    let (office, dir, floor) = floor_office("deliver-request-input");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::Codex,
        SessionState::Running,
    );
    hook(&office, &id, "PreToolUse", "call-0001", "x");
    let asked = || vec![row(&id, "call-0001")];
    rescan(
        &office,
        &id,
        SessionState::WaitingHuman,
        asked(),
        None,
        None,
    );
    queue(&office, &floor, &id, NOTE);
    held(&office, &floor, &id, &terminal);
    hook(&office, &id, "PostToolUse", "call-0001", "x");
    rescan(
        &office,
        &id,
        SessionState::WaitingHuman,
        asked(),
        None,
        None,
    );
    held(&office, &floor, &id, &terminal);
    hook(&office, &id, "Stop", "", "");
    rescan(&office, &id, SessionState::Idle, Vec::new(), None, None);
    delivered(&office, &floor, &id, &terminal);
    fs::remove_dir_all(&dir).unwrap();
}

// A turn cancelled in the terminal fires no `Stop`: the rollout's turn end retires the request
// and the hold ends. The feed used to keep the prompt's state for ever.
#[test]
fn a_request_whose_turn_the_rollout_shows_ended_no_longer_holds() {
    let (office, dir, floor) = floor_office("deliver-request-ended");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::Codex,
        SessionState::Running,
    );
    office.lock().mail.quiet = Some(Duration::from_millis(100));
    ask(&office, &id);
    rescan(&office, &id, SessionState::Running, Vec::new(), None, None);
    queue(&office, &floor, &id, NOTE);
    held(&office, &floor, &id, &terminal);
    let ended = Evidence {
        ended_turns: vec!["t1".to_string()],
        ..Evidence::default()
    };
    // The rollout's turn end is newer than every hook (its time is cut to whole seconds).
    std::thread::sleep(Duration::from_millis(1100));
    let last = Some(now_iso());
    rescan(
        &office,
        &id,
        SessionState::Idle,
        Vec::new(),
        Some(ended),
        last,
    );
    assert!(
        office.lock().feed.requests().open(&id).is_empty(),
        "retired by the rollout"
    );
    assert_ne!(hold_of(&office, &floor, &id), Some(Hold::WaitingOnYou));
    // Stage M1's silence rule then takes it as idle.
    std::thread::sleep(Duration::from_millis(1200));
    delivered(&office, &floor, &id, &terminal);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_desk_that_ended_its_turn_on_a_prose_ask_takes_the_note() {
    let (office, dir, floor) = floor_office("deliver-request-prose");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::Codex,
        SessionState::Running,
    );
    hook(&office, &id, "Stop", "", "");
    let prose = vec![row(&id, "prose")];
    rescan(&office, &id, SessionState::WaitingHuman, prose, None, None);
    {
        let inner = office.lock();
        let s = inner.found.sessions.iter().find(|s| s.id == id).unwrap();
        assert_eq!(
            s.state,
            SessionState::WaitingHuman,
            "the prose ask survives its Stop"
        );
    }
    queue(&office, &floor, &id, NOTE);
    delivered(&office, &floor, &id, &terminal);
    fs::remove_dir_all(&dir).unwrap();
}
