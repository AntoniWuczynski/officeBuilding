use super::super::tests::floor_office;
use super::*;
use crate::manager::wire::LogEntry;
use crate::model::{Project, Spend};
use crate::structured::desks::DeskInfo;
use crate::structured::{Driver, Values};
use portable_pty::CommandBuilder;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const INTRO: &str = "Note from the test: ";

pub(in super::super) fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(in super::super) fn session(
    id: &str,
    floor: &Project,
    tool: ToolKind,
    control: ControlMode,
) -> Session {
    Session {
        id: id.to_string(),
        tool,
        project_id: floor.id.clone(),
        title: format!("desk {id}"),
        state: SessionState::Idle,
        control,
        model: None,
        effort: None,
        last_activity_at: "2026-11-12T10:00:00Z".to_string(),
        pending_question_ids: Vec::new(),
        spend: Spend::default(),
        helpers: Vec::new(),
        pets: Vec::new(),
        worktree: false,
        branch: None,
    }
}

/// A desk running `/bin/cat` in an app terminal, bound to an injected session of `tool`
/// in `state`. Returns the terminal and the session id.
pub(in super::super) fn seat(
    office: &Office,
    dir: &std::path::Path,
    floor: &Project,
    tool: ToolKind,
    state: SessionState,
) -> (String, String) {
    let terminal = office
        .terminals
        .spawn(CommandBuilder::new("/bin/cat"), &dir.join("home/code/app"))
        .unwrap();
    let id = format!("s-{terminal}");
    let mut inner = office.lock();
    inner.owned.insert(terminal.clone(), id.clone());
    inner.ran_in.insert(id.clone(), terminal.clone());
    let mut s = session(&id, floor, tool, ControlMode::Full);
    s.state = state;
    inner.found.sessions.push(s);
    (terminal, id)
}

/// Drop a hook event into the feed's folder as a tool would, and collect it.
pub(in super::super) fn event(
    office: &Office,
    session_id: &str,
    name: &str,
    notification: Option<&str>,
) {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    // A newer event must carry a later time than a hand-over just before it.
    std::thread::sleep(Duration::from_millis(15));
    let dir = office.lock().feed.dir().to_path_buf();
    fs::create_dir_all(&dir).unwrap();
    let kind = notification.map_or_else(String::new, |n| format!(r#","notification_type":"{n}""#));
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    fs::write(
        dir.join(format!("{name}.{n:06}")),
        format!(r#"{{"session_id":"{session_id}"{kind}}}"#),
    )
    .unwrap();
    office.lock().feed.collect(SystemTime::now());
}

/// Queue a note as a sender would, with its log entry.
pub(in super::super) fn queue(office: &Office, floor: &Project, to: &str, text: &str) -> String {
    let mut guard = office.lock();
    let inner = &mut *guard;
    let note = NewNote {
        floor: floor.id.clone(),
        from: "boss".to_string(),
        to: to.to_string(),
        intro: INTRO.to_string(),
        text: text.to_string(),
    };
    let id = Office::post_note(inner, note).unwrap();
    let root = Office::root_key(inner, &floor.id).unwrap();
    let at = now_iso();
    inner
        .store
        .records
        .managers
        .entry(root)
        .or_default()
        .log
        .push(LogEntry {
            id: format!("g-{id}"),
            at,
            event: LogEvent::Note {
                note_id: id.clone(),
                to_session_id: to.to_string(),
                to_title: "a desk".to_string(),
                text: text.to_string(),
                state: NoteState::Queued,
            },
        });
    id
}

pub(in super::super) fn state_of(office: &Office, note: &str) -> Option<NoteState> {
    let inner = office.lock();
    inner
        .store
        .records
        .managers
        .values()
        .flat_map(|r| r.log.iter())
        .find_map(|entry| match &entry.event {
            LogEvent::Note { note_id, state, .. } if note_id == note => Some(*state),
            _ => None,
        })
}

pub(in super::super) fn hold_of(
    office: &Office,
    floor: &Project,
    session_id: &str,
) -> Option<Hold> {
    let inner = office.lock();
    let desk = office.desk_now(&inner, session_id).unwrap();
    Office::hold_for(&inner, &floor.id, &desk)
}

pub(in super::super) fn waiting(office: &Office) -> Vec<String> {
    office
        .lock()
        .mail
        .outbox
        .items()
        .iter()
        .map(|i| i.id.clone())
        .collect()
}

pub(in super::super) fn messages(office: &Office) -> Vec<AgentMessage> {
    office.lock().store.records.messages.clone()
}

pub(in super::super) fn screen(office: &Office, terminal: &str) -> String {
    crate::pty::plain_text(&office.terminals.backlog(terminal).unwrap().data)
}

pub(in super::super) fn set_paused(office: &Office, floor: &Project, paused: bool) {
    let mut guard = office.lock();
    let root = Office::root_key(&guard, &floor.id).unwrap();
    guard.store.records.managers.entry(root).or_default().paused = paused;
}

#[test]
fn an_idle_desk_gets_the_note_typed_stored_and_logged() {
    let (office, dir, floor) = floor_office("deliver-idle");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &id, "Stop", None);
    let note = queue(&office, &floor, &id, "Take the first item of TODO.md.");
    assert_eq!(hold_of(&office, &floor, &id), None);
    assert!(office.deliver_due());
    // The terminal echoes the typing, and cat prints the line back once Enter submits it.
    wait_for("the note typed and submitted", || {
        screen(&office, &terminal)
            .matches("Note from the test: Take the first item of TODO.md.")
            .count()
            == 2
    });
    let sent = messages(&office);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (
            sent[0].from_session_id.as_str(),
            sent[0].to_session_id.as_str(),
            sent[0].text.as_str()
        ),
        ("boss", id.as_str(), "Take the first item of TODO.md.")
    );
    assert_eq!(state_of(&office, &note), Some(NoteState::Delivered));
    assert!(waiting(&office).is_empty());
    assert!(!office.deliver_due(), "nothing more to do");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_busy_stuck_or_finished_desk_waits() {
    let (office, dir, floor) = floor_office("deliver-busy");
    for (state, hold) in [
        (SessionState::Running, Hold::Busy),
        (SessionState::Thinking, Hold::Busy),
        (SessionState::Error, Hold::Stuck),
        (SessionState::Done, Hold::Finished),
    ] {
        let (terminal, id) = seat(&office, &dir, &floor, ToolKind::ClaudeCode, state);
        event(&office, &id, "Stop", None);
        let note = queue(&office, &floor, &id, "go");
        assert!(!office.deliver_due(), "{state:?}");
        assert_eq!(hold_of(&office, &floor, &id), Some(hold), "{state:?}");
        assert!(waiting(&office).contains(&note));
        assert_eq!(screen(&office, &terminal), "");
    }
    // The session's files say idle but the feed's newest event says it is working.
    let (_, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &id, "PreToolUse", None);
    queue(&office, &floor, &id, "go");
    assert_eq!(hold_of(&office, &floor, &id), Some(Hold::Busy));
    assert!(!office.deliver_due());
    assert!(messages(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_desk_waiting_on_a_prompt_or_a_question_is_never_typed_into() {
    let (office, dir, floor) = floor_office("deliver-prompt");
    let (prompted, a) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &a, "Notification", Some("permission_prompt"));
    let (asked, b) = seat(&office, &dir, &floor, ToolKind::Codex, SessionState::Idle);
    event(&office, &b, "Stop", None);
    {
        let mut inner = office.lock();
        let s = inner.found.sessions.iter_mut().find(|s| s.id == b).unwrap();
        s.pending_question_ids.push("q-1".to_string());
    }
    queue(&office, &floor, &a, "go");
    queue(&office, &floor, &b, "go");
    for _ in 0..4 {
        assert!(!office.deliver_due());
        assert_eq!(hold_of(&office, &floor, &a), Some(Hold::WaitingOnYou));
        assert_eq!(hold_of(&office, &floor, &b), Some(Hold::WaitingOnYou));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(screen(&office, &prompted), "");
    assert_eq!(screen(&office, &asked), "");
    assert_eq!(waiting(&office).len(), 2);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn desks_with_no_idle_signal_or_started_by_hand_are_not_typed_into() {
    let (office, dir, floor) = floor_office("deliver-route");
    let (quiet, a) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    let (hookless, b) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::Antigravity,
        SessionState::Idle,
    );
    event(&office, &b, "Stop", None);
    let c = "by-hand".to_string();
    office.lock().found.sessions.push(session(
        &c,
        &floor,
        ToolKind::ClaudeCode,
        ControlMode::RaiseWindow,
    ));
    for to in [&a, &b, &c] {
        queue(&office, &floor, to, "go");
    }
    assert!(!office.deliver_due());
    assert_eq!(hold_of(&office, &floor, &a), Some(Hold::NoIdleSignal));
    assert_eq!(hold_of(&office, &floor, &b), Some(Hold::NoIdleSignal));
    assert_eq!(hold_of(&office, &floor, &c), Some(Hold::HandStarted));
    let inner = office.lock();
    let hand = office.desk_now(&inner, &c).unwrap();
    assert_eq!((hand.key.as_str(), hand.terminal), (c.as_str(), None));
    assert_eq!(hand.title, "desk by-hand");
    drop(inner);
    assert_eq!(screen(&office, &quiet), "");
    assert_eq!(screen(&office, &hookless), "");
    assert!(messages(&office).is_empty());
    // Once the feed reports the first desk idle, its note goes.
    event(&office, &a, "Stop", None);
    assert!(office.deliver_due());
    assert_eq!(messages(&office).len(), 1);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_paused_floor_holds_its_notes_and_resuming_hands_them_over() {
    let (office, dir, floor) = floor_office("deliver-pause");
    let (_, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &id, "Stop", None);
    set_paused(&office, &floor, true);
    let note = queue(&office, &floor, &id, "go");
    assert!(!office.deliver_due());
    assert_eq!(hold_of(&office, &floor, &id), Some(Hold::Paused));
    set_paused(&office, &floor, false);
    assert!(office.deliver_due());
    assert_eq!(state_of(&office, &note), Some(NoteState::Delivered));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_keystroke_holds_a_terminal_and_a_terminals_own_report_does_not() {
    let (office, dir, floor) = floor_office("deliver-typed");
    let (_, a) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    let (_, b) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &a, "Stop", None);
    event(&office, &b, "Stop", None);
    office.terminal_write(&a, "a").unwrap();
    office.terminal_write(&b, "\x1b[I").unwrap();
    office.terminal_write(&b, "\x1b[<0;10;5M").unwrap();
    let held = queue(&office, &floor, &a, "go");
    let sent = queue(&office, &floor, &b, "go");
    assert_eq!(hold_of(&office, &floor, &a), Some(Hold::YouTyped));
    assert!(office.deliver_due());
    assert_eq!(state_of(&office, &held), Some(NoteState::Queued));
    assert_eq!(state_of(&office, &sent), Some(NoteState::Delivered));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn enter_from_a_human_overrides_the_waiting_notes_but_not_while_answering_a_prompt() {
    let (office, dir, floor) = floor_office("deliver-override");
    let (_, busy) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Running,
    );
    event(&office, &busy, "PreToolUse", None);
    let first = queue(&office, &floor, &busy, "one");
    let second = queue(&office, &floor, &busy, "two");
    assert!(!office.deliver_due());
    office.terminal_write(&busy, "do this instead").unwrap();
    assert_eq!(waiting(&office).len(), 2, "no Enter yet");
    office.terminal_write(&busy, "\r").unwrap();
    assert!(waiting(&office).is_empty());
    assert_eq!(state_of(&office, &first), Some(NoteState::Overridden));
    assert_eq!(state_of(&office, &second), Some(NoteState::Overridden));
    assert!(office.deliver_due(), "the override shows");
    assert!(!office.deliver_due());

    let (_, asking) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &asking, "Notification", Some("permission_prompt"));
    let kept = queue(&office, &floor, &asking, "three");
    office.terminal_write(&asking, "1\r").unwrap();
    assert_eq!(waiting(&office), vec![kept.clone()]);
    assert_eq!(state_of(&office, &kept), Some(NoteState::Queued));
    assert!(!office.deliver_due());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_next_note_waits_until_the_last_one_is_picked_up() {
    let (office, dir, floor) = floor_office("deliver-turn");
    let (_, id) = seat(&office, &dir, &floor, ToolKind::Codex, SessionState::Idle);
    event(&office, &id, "Stop", None);
    let first = queue(&office, &floor, &id, "one");
    let second = queue(&office, &floor, &id, "two");
    assert!(office.deliver_due());
    assert_eq!(state_of(&office, &first), Some(NoteState::Delivered));
    assert_eq!(hold_of(&office, &floor, &id), Some(Hold::AwaitingTurn));
    assert!(!office.deliver_due());
    assert_eq!(waiting(&office), vec![second.clone()]);
    event(&office, &id, "Stop", None);
    assert!(office.deliver_due());
    assert_eq!(state_of(&office, &second), Some(NoteState::Delivered));
    assert_eq!(messages(&office).len(), 2);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_note_for_a_desk_that_left_is_dropped() {
    let (office, dir, floor) = floor_office("deliver-left");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Running,
    );
    let note = queue(&office, &floor, &id, "go");
    assert!(!office.deliver_due());
    office.terminals.write(&terminal, "\u{4}").unwrap();
    office
        .terminals
        .wait_exit(&terminal, Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert!(office.deliver_due(), "a dropped note shows");
    assert_eq!(state_of(&office, &note), Some(NoteState::Dropped));
    assert!(waiting(&office).is_empty());
    assert!(!office.deliver_due());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn two_passes_at_once_type_a_note_once() {
    let (office, dir, floor) = floor_office("deliver-race");
    let (terminal, id) = seat(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Idle,
    );
    event(&office, &id, "Stop", None);
    queue(&office, &floor, &id, "only once please");
    let handed: Vec<bool> = std::thread::scope(|scope| {
        let passes: Vec<_> = (0..2)
            .map(|_| scope.spawn(|| office.deliver_due()))
            .collect();
        passes.into_iter().map(|p| p.join().unwrap()).collect()
    });
    assert!(handed.contains(&true));
    assert_eq!(messages(&office).len(), 1);
    wait_for("the note on the screen", || {
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
    fs::remove_dir_all(&dir).unwrap();
}

/// Records the prompts a structured desk is given.
struct Recorder(Arc<Mutex<Vec<String>>>);

impl Driver for Recorder {
    fn prompt(&self, text: &str) -> Result<(), String> {
        crate::sync::lock(&self.0).push(text.to_string());
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

#[test]
fn a_structured_desk_gets_its_note_as_a_turn_only_when_idle() {
    let (office, dir, floor) = floor_office("deliver-structured");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let seat_desk = |state| {
        let info = DeskInfo {
            tool: ToolKind::Codex,
            project_id: floor.id.clone(),
            root: dir.join("home/code/app"),
            title: "structured".to_string(),
            model: "default".to_string(),
            effort: "default".to_string(),
        };
        let driver = Box::new(Recorder(Arc::clone(&prompts)));
        office.structured.seat("structured-9", info, state, driver);
    };
    seat_desk(SessionState::Idle);
    let first = queue(&office, &floor, "structured-9", "one");
    assert!(office.deliver_due());
    assert_eq!(
        *crate::sync::lock(&prompts),
        ["Note from the test: one".to_string()]
    );
    assert_eq!(state_of(&office, &first), Some(NoteState::Delivered));
    assert_eq!(messages(&office)[0].to_session_id, "structured-9");

    seat_desk(SessionState::Thinking);
    let second = queue(&office, &floor, "structured-9", "two");
    assert!(!office.deliver_due());
    assert_eq!(hold_of(&office, &floor, "structured-9"), Some(Hold::Busy));

    office.chat_send("structured-9", "mine").unwrap();
    assert_eq!(state_of(&office, &second), Some(NoteState::Overridden));
    assert!(waiting(&office).is_empty());
    assert_eq!(crate::sync::lock(&prompts).len(), 1);
    let queued = office.snapshot().queued;
    assert_eq!(queued.len(), 1);
    assert_eq!(
        (queued[0].text.as_str(), queued[0].hold),
        ("mine", Hold::Busy)
    );

    seat_desk(SessionState::Idle);
    assert!(office.deliver_due());
    assert_eq!(crate::sync::lock(&prompts)[1], "mine");
    fs::remove_dir_all(&dir).unwrap();
}
