use super::super::super::deliver::tests::{
    event, hold_of, messages, queue, screen, seat, session, set_paused, state_of, wait_for, waiting,
};
use super::super::super::tests::floor_office;
use super::*;
use crate::manager::wire::NoteState;
use crate::model::{ControlMode, Project};
use crate::structured::desks::DeskInfo;
use crate::structured::{Driver, Values};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A Claude Code desk in a `/bin/cat` terminal whose feed last said `last`.
fn claude(office: &Office, dir: &Path, floor: &Project, last: &str) -> (String, String) {
    let (terminal, id) = seat(office, dir, floor, ToolKind::ClaudeCode, SessionState::Idle);
    event(office, &id, last, None);
    (terminal, id)
}

fn queued(office: &Office) -> Vec<QueuedNote> {
    office.snapshot().queued
}

/// What the terminal received, escape bytes included.
fn raw(office: &Office, terminal: &str) -> String {
    office.terminals.backlog(terminal).unwrap().data
}

/// A structured desk's driver that records its calls and can refuse prompts.
struct Fake {
    calls: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl Driver for Fake {
    fn prompt(&self, text: &str) -> Result<(), String> {
        crate::sync::lock(&self.calls).push(format!("prompt {text}"));
        if self.fail {
            Err("the agent is gone".to_string())
        } else {
            Ok(())
        }
    }
    fn answer(&self, _: &str, _: &str, _: &Values) -> Result<(), String> {
        Ok(())
    }
    fn cancel(&self) -> Result<(), String> {
        crate::sync::lock(&self.calls).push("cancel".to_string());
        Ok(())
    }
    fn stop(&self) {
        crate::sync::lock(&self.calls).push("stop".to_string());
    }
}

/// Seat (or reseat) the structured desk `structured-7` and return its driver's calls.
fn structured(
    office: &Office,
    dir: &Path,
    floor: &Project,
    state: SessionState,
    fail: bool,
) -> Arc<Mutex<Vec<String>>> {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let info = DeskInfo {
        tool: ToolKind::Codex,
        project_id: floor.id.clone(),
        root: dir.join("home/code/app"),
        title: "structured".to_string(),
        model: "default".to_string(),
        effort: "default".to_string(),
    };
    let driver = Box::new(Fake {
        calls: Arc::clone(&calls),
        fail,
    });
    office.structured.seat("structured-7", info, state, driver);
    calls
}

#[test]
fn an_idle_desk_gets_the_message_typed_with_no_introduction_and_nothing_stored() {
    let (office, dir, floor) = floor_office("chat-idle");
    let (terminal, id) = claude(&office, &dir, &floor, "Stop");
    office.chat_send(&id, "Read the README first.").unwrap();
    // The terminal echoes the typing, and cat prints the line back once Enter submits it.
    wait_for("the message typed and submitted", || {
        screen(&office, &terminal)
            .matches("Read the README first.")
            .count()
            == 2
    });
    assert!(!screen(&office, &terminal).contains("Note from"));
    assert!(messages(&office).is_empty());
    assert!(queued(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_busy_desk_shows_the_message_as_waiting_until_its_turn_ends() {
    let (office, dir, floor) = floor_office("chat-busy");
    let (terminal, id) = claude(&office, &dir, &floor, "PreToolUse");
    office.chat_send(&id, "later please").unwrap();
    let waiting = queued(&office);
    assert_eq!(waiting.len(), 1);
    assert_eq!(
        (
            waiting[0].to_session_id.as_str(),
            waiting[0].text.as_str(),
            waiting[0].hold
        ),
        (id.as_str(), "later please", Hold::Busy)
    );
    assert_eq!(screen(&office, &terminal), "");
    event(&office, &id, "Stop", None);
    assert!(office.deliver_due());
    wait_for("the message typed", || {
        screen(&office, &terminal).contains("later please")
    });
    assert!(queued(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_message_overrides_waiting_notes_and_a_later_note_goes_after_it() {
    let (office, dir, floor) = floor_office("chat-override");
    let (terminal, id) = claude(&office, &dir, &floor, "PreToolUse");
    let before = queue(&office, &floor, &id, "the boss first");
    office.chat_send(&id, "mine goes first").unwrap();
    assert_eq!(state_of(&office, &before), Some(NoteState::Overridden));
    let after = queue(&office, &floor, &id, "the boss after");
    event(&office, &id, "Stop", None);
    assert!(office.deliver_due());
    assert_eq!(state_of(&office, &after), Some(NoteState::Queued));
    event(&office, &id, "Stop", None);
    assert!(office.deliver_due());
    assert_eq!(state_of(&office, &after), Some(NoteState::Delivered));
    wait_for("both typed", || {
        screen(&office, &terminal).contains("the boss after")
    });
    let shown = screen(&office, &terminal);
    assert!(!shown.contains("the boss first"));
    assert!(shown.find("mine goes first") < shown.find("the boss after"));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_paused_manager_does_not_hold_a_humans_message() {
    let (office, dir, floor) = floor_office("chat-paused");
    let (terminal, id) = claude(&office, &dir, &floor, "Stop");
    set_paused(&office, &floor, true);
    office.chat_send(&id, "paused or not").unwrap();
    wait_for("the message typed", || {
        screen(&office, &terminal).contains("paused or not")
    });
    assert!(queued(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_open_permission_prompt_holds_the_message() {
    let (office, dir, floor) = floor_office("chat-prompt");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &id, "Notification", Some("permission_prompt"));
    office.chat_send(&id, "do not answer it").unwrap();
    for _ in 0..3 {
        assert!(!office.deliver_due());
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(queued(&office)[0].hold, Hold::WaitingOnYou);
    assert_eq!(screen(&office, &terminal), "");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_unsent_draft_holds_the_message_until_it_is_sent() {
    let (office, dir, floor) = floor_office("chat-draft");
    let (terminal, id) = claude(&office, &dir, &floor, "Stop");
    office.terminal_write(&id, "half a thought").unwrap();
    office.chat_send(&id, "from the chat").unwrap();
    assert_eq!(queued(&office)[0].hold, Hold::YouTyped);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!screen(&office, &terminal).contains("from the chat"));
    office.terminal_write(&id, "\r").unwrap();
    event(&office, &id, "Stop", None);
    assert!(office.deliver_due());
    wait_for("the message typed", || {
        screen(&office, &terminal).contains("from the chat")
    });
    assert!(queued(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_humans_own_recent_enter_does_not_hold_his_message() {
    let (office, dir, floor) = floor_office("chat-enter");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    office.terminal_write(&id, "x\r").unwrap();
    event(&office, &id, "Stop", None);
    queue(&office, &floor, &id, "the boss");
    assert_eq!(hold_of(&office, &floor, &id), Some(Hold::YouTyped));
    office.chat_send(&id, "right away").unwrap();
    wait_for("the message typed", || {
        screen(&office, &terminal).contains("right away")
    });
    assert!(queued(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_waiting_message_can_be_withdrawn_once_and_a_managers_note_cannot() {
    let (office, dir, floor) = floor_office("chat-withdraw");
    let (_, id) = claude(&office, &dir, &floor, "PreToolUse");
    office.chat_send(&id, "never mind").unwrap();
    let message = queued(&office)[0].id.clone();
    office.withdraw_message(&message).unwrap();
    assert!(queued(&office).is_empty());
    assert_eq!(
        office.withdraw_message(&message),
        Err("that message is no longer waiting".to_string())
    );
    let note = queue(&office, &floor, &id, "the boss");
    assert!(office.withdraw_message(&note).is_err());
    assert_eq!(waiting(&office), vec![note]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_sixth_waiting_message_for_a_desk_is_refused() {
    let (office, dir, floor) = floor_office("chat-cap");
    let (_, id) = claude(&office, &dir, &floor, "PreToolUse");
    for n in 0..5 {
        office.chat_send(&id, &format!("message {n}")).unwrap();
    }
    assert_eq!(
        office.chat_send(&id, "one too many"),
        Err("that desk already has 5 notes waiting".to_string())
    );
    assert_eq!(queued(&office).len(), 5);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_empty_or_overlong_message_is_refused() {
    let (office, dir, floor) = floor_office("chat-clean");
    let (_, id) = claude(&office, &dir, &floor, "PreToolUse");
    assert_eq!(
        office.chat_send(&id, " \n\t\u{200b}"),
        Err("there is nothing to send".to_string())
    );
    assert_eq!(
        office.chat_send(&id, &"a".repeat(MESSAGE_MAX + 1)),
        Err("that is longer than 16000 characters".to_string())
    );
    assert!(queued(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_hand_started_session_refuses_a_message() {
    let (office, dir, floor) = floor_office("chat-by-hand");
    office.lock().found.sessions.push(session(
        "by-hand",
        &floor,
        ToolKind::ClaudeCode,
        ControlMode::RaiseWindow,
    ));
    assert_eq!(
        office.chat_send("by-hand", "hello"),
        Err(
            "this session was started outside the building, so nothing can be sent to it from here"
                .to_string()
        )
    );
    assert_eq!(
        office.chat_send("nobody", "hello"),
        Err("that desk has left, so there is nobody to send it to".to_string())
    );
    assert!(queued(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_failed_prompt_keeps_the_message_first_and_the_next_pass_delivers_it() {
    let (office, dir, floor) = floor_office("chat-retry");
    let failing = structured(&office, &dir, &floor, SessionState::Idle, true);
    office.chat_send("structured-7", "try me").unwrap();
    let first = queued(&office);
    assert_eq!(first.len(), 1);
    assert!(office.deliver_due(), "a failed hand-over shows");
    assert_eq!(queued(&office), first);
    assert_eq!(crate::sync::lock(&failing).len(), 2, "one try a pass");
    let working = structured(&office, &dir, &floor, SessionState::Idle, false);
    assert!(office.deliver_due());
    assert_eq!(*crate::sync::lock(&working), ["prompt try me".to_string()]);
    assert!(queued(&office).is_empty());
    assert!(messages(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_message_for_a_desk_whose_terminal_exited_is_dropped() {
    let (office, dir, floor) = floor_office("chat-left");
    let (terminal, id) = claude(&office, &dir, &floor, "PreToolUse");
    office.chat_send(&id, "too late").unwrap();
    office.terminals.write(&terminal, "\u{4}").unwrap();
    office
        .terminals
        .wait_exit(&terminal, Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert!(office.deliver_due(), "a dropped message shows");
    assert!(office.lock().mail.own.items().is_empty());
    assert!(!office.deliver_due());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stop_sends_one_escape_and_a_second_stop_soon_after_sends_nothing() {
    let (office, dir, floor) = floor_office("chat-stop");
    let (terminal, id) = claude(&office, &dir, &floor, "PreToolUse");
    office.chat_interrupt(&id).unwrap();
    wait_for("the Escape", || {
        let data = raw(&office, &terminal);
        data.contains('\x1b') || data.contains("^[")
    });
    std::thread::sleep(Duration::from_millis(100));
    let once = raw(&office, &terminal);
    office.chat_interrupt(&id).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(raw(&office, &terminal), once);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stop_is_refused_on_a_prompt_ignored_when_idle_and_only_for_claude_code() {
    let (office, dir, floor) = floor_office("chat-stop-refused");
    let (prompted, a) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &a, "Notification", Some("permission_prompt"));
    assert_eq!(
        office.chat_interrupt(&a),
        Err(
            "it is waiting for your answer, and Escape would decline it: answer it in the terminal"
                .to_string()
        )
    );
    let (idle, b) = claude(&office, &dir, &floor, "Stop");
    assert_eq!(office.chat_interrupt(&b), Ok(()));
    let (codex, c) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::Codex,
        SessionState::Running,
    );
    assert_eq!(
        office.chat_interrupt(&c),
        Err(
            "only a Claude Code desk in one of the building's terminals can be stopped from its chat"
                .to_string()
        )
    );
    std::thread::sleep(Duration::from_millis(200));
    for terminal in [prompted, idle, codex] {
        assert_eq!(raw(&office, &terminal), "");
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stop_on_a_structured_desk_cancels_its_turn_and_never_ends_it() {
    let (office, dir, floor) = floor_office("chat-stop-structured");
    let calls = structured(&office, &dir, &floor, SessionState::Thinking, false);
    office.chat_interrupt("structured-7").unwrap();
    assert_eq!(*crate::sync::lock(&calls), ["cancel".to_string()]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn after_a_stop_an_idle_desk_takes_a_humans_message_but_not_a_managers_note() {
    let (office, dir, floor) = floor_office("chat-stopped");
    let (terminal, id) = claude(&office, &dir, &floor, "PreToolUse");
    office.chat_interrupt(&id).unwrap();
    office.chat_send(&id, "after the stop").unwrap();
    wait_for("the message typed", || {
        screen(&office, &terminal).contains("after the stop")
    });
    queue(&office, &floor, &id, "the boss");
    assert_eq!(hold_of(&office, &floor, &id), Some(Hold::Busy));
    event(&office, &id, "PreToolUse", None);
    office.chat_send(&id, "once it works again").unwrap();
    assert_eq!(queued(&office)[0].hold, Hold::Busy);
    assert_eq!(hold_of(&office, &floor, &id), Some(Hold::Busy));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn two_passes_at_once_type_a_message_once() {
    let (office, dir, floor) = floor_office("chat-race");
    let (terminal, id) = claude(&office, &dir, &floor, "PreToolUse");
    office.chat_send(&id, "only once please").unwrap();
    event(&office, &id, "Stop", None);
    let handed: Vec<bool> = std::thread::scope(|scope| {
        let passes: Vec<_> = (0..2)
            .map(|_| scope.spawn(|| office.deliver_due()))
            .collect();
        passes.into_iter().map(|p| p.join().unwrap()).collect()
    });
    assert!(handed.contains(&true));
    wait_for("the message on the screen", || {
        screen(&office, &terminal).contains("only once please")
    });
    std::thread::sleep(Duration::from_millis(300));
    // The terminal echoes the line and cat prints it back: twice for one typing.
    assert!(
        screen(&office, &terminal)
            .matches("only once please")
            .count()
            <= 2
    );
    assert!(queued(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}
