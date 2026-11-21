//! A chat desk's open requests in the building (spec 4.7, 4.8): the queue, the pending ids,
//! the alert state and the delivery gate, with a seated desk and a fake driver. Approvals
//! and notes are placeholders.

use super::super::deliver::NewNote;
use super::super::deliver::tests::session;
use super::super::tests::floor_office;
use super::*;
use crate::delivery::gate::Hold;
use crate::model::{AnswerVia, Handoff, ToolKind};
use crate::structured::{Approval, Driver, Event, Values};
use std::sync::{Arc, Mutex};

const DESK: &str = "structured-1";
/// The chat desk's own conversation id, as Codex names its thread.
const THREAD: &str = "00000000-0000-0000-0000-000000000009";

/// A driver that records what it was asked to do.
#[derive(Default)]
struct Recorder {
    answers: Mutex<Vec<String>>,
    prompts: Mutex<Vec<String>>,
}

struct Fake(Arc<Recorder>);

impl Driver for Fake {
    fn prompt(&self, text: &str) -> Result<(), String> {
        crate::sync::lock(&self.0.prompts).push(text.to_string());
        Ok(())
    }
    fn answer(&self, approval_id: &str, _: &str, _: &Values) -> Result<(), String> {
        crate::sync::lock(&self.0.answers).push(approval_id.to_string());
        Ok(())
    }
    fn cancel(&self) -> Result<(), String> {
        Ok(())
    }
    fn stop(&self) {}
}

/// An office with a Codex chat desk seated in `state` on its floor.
fn seated(tag: &str, state: SessionState) -> (Office, std::path::PathBuf, Project, Arc<Recorder>) {
    let (office, dir, floor) = floor_office(tag);
    let recorder = Arc::new(Recorder::default());
    let info = DeskInfo {
        tool: ToolKind::Codex,
        project_id: floor.id.clone(),
        root: dir.join("home/code/app"),
        title: "TASK-ONE".to_string(),
        model: "default".to_string(),
        effort: "default".to_string(),
    };
    office
        .structured
        .seat(DESK, info, state, Box::new(Fake(Arc::clone(&recorder))));
    office.refresh();
    (office, dir, floor, recorder)
}

fn ask(office: &Office, id: &str, background: bool) {
    office.structured.on_event(
        DESK,
        Event::Approval(Approval {
            id: id.to_string(),
            title: format!("ASK-{id}"),
            background,
            ..Approval::default()
        }),
    );
}

fn resolve(office: &Office, id: &str) {
    office.structured.on_event(
        DESK,
        Event::Resolved {
            approval_id: id.to_string(),
        },
    );
}

fn rows_of(office: &Office, session_id: &str) -> Vec<Question> {
    office
        .snapshot()
        .questions
        .into_iter()
        .filter(|q| q.session_id == session_id)
        .collect()
}

fn alert_state_of(office: &Office, id: &str) -> Option<SessionState> {
    office
        .desk_states()
        .into_iter()
        .find(|d| d.id == id)
        .map(|d| d.state)
}

#[test]
fn a_desk_with_two_requests_has_two_rows_on_its_floor_and_both_ids() {
    let (office, dir, floor, _) = seated("chat-two-rows", SessionState::WaitingHuman);
    ask(&office, "1", false);
    ask(&office, "2", false);
    let snapshot = office.snapshot();
    let rows: Vec<&Question> = snapshot
        .questions
        .iter()
        .filter(|q| q.session_id == DESK)
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|q| q.project_id == floor.id));
    assert!(rows.iter().all(|q| q.answer_via == AnswerVia::Chat));
    let desk = snapshot
        .sessions
        .iter()
        .find(|s| s.id == DESK)
        .expect("the desk");
    let row_ids: Vec<&str> = rows.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(desk.pending_question_ids, row_ids);
    assert_eq!(
        alert_state_of(&office, DESK),
        Some(SessionState::WaitingHuman)
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_row_answered_from_decisions_reaches_its_own_request() {
    let (office, dir, _, recorder) = seated("chat-answer", SessionState::WaitingHuman);
    ask(&office, "1", false);
    ask(&office, "s:2", false);
    let row = rows_of(&office, DESK)
        .into_iter()
        .find(|q| q.id.ends_with(":s:2"))
        .expect("the row");
    let Some(Handoff::Chat { approval_id }) = row.handoff else {
        panic!("a chat row hands off to its request: {:?}", row.handoff);
    };
    office
        .structured_answer(DESK, &approval_id, "accept", &Values::new())
        .expect("answered");
    assert_eq!(*crate::sync::lock(&recorder.answers), ["s:2"]);
    resolve(&office, &approval_id);
    let left: Vec<String> = rows_of(&office, DESK).into_iter().map(|q| q.id).collect();
    assert_eq!(left, ["structured-1:request:1"]);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_background_request_keeps_the_desk_working_and_still_alerts() {
    let (office, dir, floor, _) = seated("chat-background", SessionState::Running);
    ask(&office, "1", true);
    let snapshot = office.snapshot();
    let desk = snapshot
        .sessions
        .iter()
        .find(|s| s.id == DESK)
        .expect("desk");
    assert_eq!(desk.state, SessionState::Running);
    assert_eq!(rows_of(&office, DESK)[0].priority, 1);
    assert_eq!(
        alert_state_of(&office, DESK),
        Some(SessionState::WaitingHuman)
    );
    // The rule is for chat desks: a terminal's open row leaves its working state alone.
    let mut terminal = session("s-terminal", &floor, ToolKind::Codex, ControlMode::Full);
    terminal.state = SessionState::Running;
    terminal.pending_question_ids = vec!["s-terminal:call-1".to_string()];
    office.lock().found.sessions.push(terminal);
    assert_eq!(
        alert_state_of(&office, "s-terminal"),
        Some(SessionState::Running)
    );
    resolve(&office, "1");
    assert_eq!(alert_state_of(&office, DESK), Some(SessionState::Running));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn no_note_reaches_a_chat_desk_while_a_request_is_open() {
    let (office, dir, floor, recorder) = seated("chat-gate", SessionState::Running);
    ask(&office, "1", true);
    {
        let inner = office.lock();
        let desk = office.desk_now(&inner, DESK).expect("at its desk");
        assert!(desk.desk.prompt_open);
        assert_eq!(
            Office::hold_for(&inner, &floor.id, &desk),
            Some(Hold::WaitingOnYou)
        );
        assert_eq!(Office::own_hold(&desk), Some(Hold::WaitingOnYou));
    }
    let note = NewNote {
        floor: floor.id.clone(),
        from: "manager".to_string(),
        to: DESK.to_string(),
        intro: "INTRO: ".to_string(),
        text: "NOTE-ONE".to_string(),
    };
    Office::post_note(&mut office.lock(), note).expect("queued");
    // Idle with the request still open: it still waits.
    office
        .structured
        .on_event(DESK, Event::State(SessionState::Idle));
    office.deliver_due();
    assert!(crate::sync::lock(&recorder.prompts).is_empty());
    resolve(&office, "1");
    office.deliver_due();
    assert_eq!(*crate::sync::lock(&recorder.prompts), ["INTRO: NOTE-ONE"]);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn rows_of_the_conversation_a_chat_desk_hides_are_left_out() {
    let (office, dir, floor, _) = seated("chat-hidden", SessionState::WaitingHuman);
    office
        .structured
        .on_event(DESK, Event::SessionId(THREAD.to_string()));
    ask(&office, "1", false);
    let row = |session_id: &str, call: &str| Question {
        id: format!("{session_id}:{call}"),
        project_id: floor.id.clone(),
        session_id: session_id.to_string(),
        prompt: "QUESTION-ONE".to_string(),
        options: Vec::new(),
        allow_other: true,
        answer_via: AnswerVia::Terminal,
        priority: 0,
        asked_at: "2026-11-20T10:00:00Z".to_string(),
        context: String::new(),
        source_file: None,
        handoff: None,
    };
    {
        let mut inner = office.lock();
        inner.found.questions.push(row(THREAD, "call-1"));
        inner.found.questions.push(row("s-other", "call-2"));
    }
    let ids: Vec<String> = office
        .snapshot()
        .questions
        .into_iter()
        .map(|q| q.id)
        .collect();
    assert_eq!(ids, ["s-other:call-2", "structured-1:request:1"]);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_stored_call_written_before_handoffs_loads() {
    let records: crate::store::Records = serde_json::from_value(serde_json::json!({
        "calls": [{
            "id": "call-1", "projectId": "p", "sessionId": "", "prompt": "QUESTION-ONE",
            "options": [], "allowOther": true, "answerVia": "store", "priority": 0,
            "askedAt": "2026-11-20T10:00:00Z", "context": "", "sourceFile": null
        }]
    }))
    .expect("an old store loads");
    assert_eq!(records.calls[0].handoff, None);
}
