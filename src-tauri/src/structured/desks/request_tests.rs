//! A desk with several requests open (spec 4.7): kept in order, replaced in place, cleared
//! with the agent, and published as queue rows and pending ids. Approvals are placeholders.

use super::*;
use crate::structured::{Choice, Values};
use std::path::Path;

const DESK: &str = "structured-1";

struct Quiet;

impl Driver for Quiet {
    fn prompt(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn answer(&self, _: &str, _: &str, _: &Values) -> Result<(), String> {
        Ok(())
    }
    fn cancel(&self) -> Result<(), String> {
        Ok(())
    }
    fn stop(&self) {}
}

fn seated(state: SessionState) -> (Arc<Desks>, Arc<Mutex<Vec<Update>>>) {
    let updates = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&updates);
    let desks = Desks::new(Arc::new(move |u| lock(&seen).push(u)));
    let info = DeskInfo {
        tool: ToolKind::Codex,
        project_id: "p-1".to_string(),
        root: PathBuf::from("/work/app"),
        title: "TASK-ONE".to_string(),
        model: "default".to_string(),
        effort: "default".to_string(),
    };
    desks.seat(DESK, info, state, Box::new(Quiet));
    (desks, updates)
}

fn approval(id: &str, title: &str, detail: &str) -> Approval {
    Approval {
        id: id.to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        choices: vec![Choice {
            id: "accept".to_string(),
            label: "Allow".to_string(),
            allow: true,
        }],
        ..Approval::default()
    }
}

fn ids(approvals: &[Approval]) -> Vec<&str> {
    approvals.iter().map(|a| a.id.as_str()).collect()
}

fn resolved(id: &str) -> Event {
    Event::Resolved {
        approval_id: id.to_string(),
    }
}

#[test]
fn a_reopened_panel_lists_only_the_requests_still_open() {
    let (desks, _) = seated(SessionState::WaitingHuman);
    desks.on_event(DESK, Event::Approval(approval("1", "ASK-ONE", "")));
    desks.on_event(DESK, Event::Approval(approval("2", "ASK-TWO", "")));
    desks.on_event(DESK, resolved("2"));
    let backlog = desks.backlog(DESK).expect("backlog");
    assert_eq!(ids(&backlog.approvals), ["1"]);
}

#[test]
fn a_request_sent_again_replaces_itself_and_keeps_its_time_and_place() {
    let (desks, updates) = seated(SessionState::WaitingHuman);
    desks.on_event(DESK, Event::Approval(approval("1", "ASK-ONE", "")));
    desks.on_event(DESK, Event::Approval(approval("2", "ASK-TWO", "")));
    let first_at = lock(&desks.live)[DESK].approvals[0].at;
    let mut failed = approval("1", "ASK-ONE", "");
    failed.failed = Some("REASON".to_string());
    desks.on_event(DESK, Event::Approval(failed.clone()));
    let live = lock(&desks.live);
    assert_eq!(live[DESK].approvals.len(), 2);
    assert_eq!(live[DESK].approvals[0].at, first_at);
    assert_eq!(live[DESK].approvals[0].approval, failed);
    drop(live);
    let last = lock(&updates).last().cloned().expect("an update");
    assert_eq!(ids(&last.approvals), ["1", "2"]);
    assert_eq!(last.approvals[0].failed.as_deref(), Some("REASON"));
}

#[test]
fn the_desk_redraws_when_its_open_requests_change_and_not_for_an_entry() {
    let (desks, updates) = seated(SessionState::WaitingHuman);
    let mut failed = approval("1", "ASK-ONE", "");
    failed.failed = Some("REASON".to_string());
    let steps = [
        (Event::Approval(approval("1", "ASK-ONE", "")), true),
        (Event::Approval(approval("2", "ASK-TWO", "")), true),
        (
            Event::Entry(Entry::Agent {
                id: "m1".to_string(),
                text: "TEXT".to_string(),
            }),
            false,
        ),
        (Event::Approval(failed), true),
        // The same request again, nothing the building shows has changed.
        (Event::Approval(approval("2", "ASK-TWO-AGAIN", "")), false),
        (resolved("2"), true),
        (resolved("9"), false),
    ];
    for (n, (event, redraw)) in steps.into_iter().enumerate() {
        desks.on_event(DESK, event);
        let last = lock(&updates).last().map(|u| u.desk_changed);
        assert_eq!(last, Some(redraw), "step {n}");
    }
}

#[test]
fn an_approval_leaves_the_state_to_the_driver() {
    let (desks, _) = seated(SessionState::Running);
    desks.on_event(DESK, Event::Approval(approval("1", "ASK-ONE", "")));
    assert_eq!(desks.sessions()[0].state, SessionState::Running);
    desks.on_event(DESK, Event::State(SessionState::Running));
    assert_eq!(desks.sessions()[0].state, SessionState::Running);
    desks.on_event(DESK, Event::State(SessionState::WaitingHuman));
    assert_eq!(desks.sessions()[0].state, SessionState::WaitingHuman);
}

#[test]
fn an_exit_clears_every_request_and_a_resume_starts_with_none() {
    let (desks, updates) = seated(SessionState::WaitingHuman);
    desks.on_event(DESK, Event::Approval(approval("1", "ASK-ONE", "")));
    desks.on_event(DESK, Event::Approval(approval("2", "ASK-TWO", "")));
    desks.on_event(
        DESK,
        Event::Exited {
            code: Some(0),
            message: None,
        },
    );
    assert!(lock(&updates).last().expect("update").approvals.is_empty());
    assert!(desks.backlog(DESK).expect("backlog").approvals.is_empty());
    // A request reported after the exit is kept on the ended desk until it comes back.
    desks.on_event(DESK, Event::Approval(approval("3", "ASK-THREE", "")));
    assert!(desks.questions().is_empty(), "an ended desk has no rows");
    let launch = crate::setup::Launch::default();
    let spawn = Spawn {
        // A tool with no driver: the resume fails at once, after the desk was reset.
        tool: ToolKind::ClaudeCode,
        cwd: Path::new("/work/app"),
        model: None,
        effort: None,
        launch: &launch,
        task: "",
        resume: Some("00000000-0000-0000-0000-000000000001"),
    };
    assert!(desks.resume(DESK, &spawn).is_err());
    assert!(desks.backlog(DESK).expect("backlog").approvals.is_empty());
}

#[test]
fn each_open_request_is_a_pending_id_and_a_row_of_the_queue() {
    let (desks, _) = seated(SessionState::Running);
    desks.on_event(
        DESK,
        Event::Approval(approval("7", "ASK-ONE\nMORE", "DETAIL-ONE\nMORE")),
    );
    let mut background = approval("s:8", "ASK-TWO", "");
    background.background = true;
    background.failed = Some("REASON".to_string());
    desks.on_event(DESK, Event::Approval(background));
    assert_eq!(
        desks.sessions()[0].pending_question_ids,
        ["structured-1:request:7", "structured-1:request:s:8"]
    );
    let rows = desks.questions();
    assert_eq!(rows.len(), 2);
    let first = &rows[0];
    assert_eq!(first.id, request_row(DESK, "7"));
    assert_eq!(
        (
            first.project_id.as_str(),
            first.session_id.as_str(),
            first.prompt.as_str()
        ),
        ("p-1", DESK, "ASK-ONE DETAIL-ONE")
    );
    assert!(first.options.is_empty());
    assert!(!first.allow_other);
    assert_eq!(first.answer_via, AnswerVia::Chat);
    assert_eq!(first.priority, 0);
    assert_eq!(first.context, "");
    assert_eq!(first.source_file, None);
    assert_eq!(
        first.handoff,
        Some(Handoff::Chat {
            approval_id: "7".to_string()
        })
    );
    assert!(first.asked_at.ends_with('Z'));
    let second = &rows[1];
    assert_eq!(second.prompt, "ASK-TWO");
    assert_eq!(second.priority, 1);
    assert_eq!(
        second.context,
        "Your answer did not reach the agent: REASON. Open the request to send it again."
    );
}

#[test]
fn a_long_request_is_cut_in_its_row() {
    let (desks, _) = seated(SessionState::WaitingHuman);
    desks.on_event(
        DESK,
        Event::Approval(approval("1", &"X".repeat(300), &"Y".repeat(300))),
    );
    assert_eq!(desks.questions()[0].prompt.chars().count(), 400);
}
