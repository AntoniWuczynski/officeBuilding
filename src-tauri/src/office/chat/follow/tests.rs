use super::super::super::deliver::tests::{event, seat, session};
use super::super::super::tests::floor_office;
use super::*;
use crate::model::Project;
use crate::structured::desks::DeskInfo;
use crate::structured::{Driver, Values};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

// Invented lines in the shapes the transcript reader maps (see
// discovery/claude_code/chat/tests.rs). The transcript format is not a contract.
const PROMPT: &str = r#"{"type":"user","message":{"role":"user","content":"fix the parser"}}"#;
const SAY: &str = r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"Running the tests first."}]}}"#;
const BASH: &str = r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"pnpm test"}}]}}"#;
const DONE: &str = r#"{"type":"assistant","message":{"id":"m3","content":[{"type":"text","text":"Fixed: the last token is kept."}]}}"#;

fn transcript(dir: &Path, session_id: &str) -> PathBuf {
    let folder = dir.join("home/.claude/projects/-code-app");
    fs::create_dir_all(&folder).unwrap();
    folder.join(format!("{session_id}.jsonl"))
}

fn text(lines: &[&str]) -> String {
    lines.iter().flat_map(|l| [*l, "\n"]).collect()
}

fn append(path: &Path, lines: &[&str]) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    file.write_all(text(lines).as_bytes()).unwrap();
}

/// A Claude Code desk in an app terminal whose transcript holds `lines`. Its own files
/// are old, so the feed's events set its state. Returns the session id and the transcript.
fn desk(office: &Office, dir: &Path, floor: &Project, lines: &[&str]) -> (String, PathBuf) {
    let (_, id) = seat(office, dir, floor, ToolKind::ClaudeCode, SessionState::Idle);
    old(office, &id);
    let path = transcript(dir, &id);
    if !lines.is_empty() {
        append(&path, lines);
    }
    (id, path)
}

fn old(office: &Office, session_id: &str) {
    let mut inner = office.lock();
    if let Some(s) = inner.found.sessions.iter_mut().find(|s| s.id == session_id) {
        s.last_activity_at = "2000-01-01T00:00:00Z".to_string();
    }
}

/// Let the collected events set the sessions' states, as a rescan does.
fn apply_feed(office: &Office) {
    let mut guard = office.lock();
    let inner = &mut *guard;
    inner.feed.apply(&mut inner.found.sessions);
}

/// Drop a Notification carrying `message` into the feed, collect it and apply it.
fn notify(office: &Office, session_id: &str, kind: &str, message: &str) {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    std::thread::sleep(Duration::from_millis(15));
    let dir = office.lock().feed.dir().to_path_buf();
    fs::create_dir_all(&dir).unwrap();
    let json = serde_json::json!({
        "session_id": session_id,
        "notification_type": kind,
        "message": message,
    });
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    fs::write(dir.join(format!("Notification.n{n:06}")), json.to_string()).unwrap();
    office.lock().feed.collect(SystemTime::now());
    apply_feed(office);
}

/// The kind of each entry, with its text or title.
fn shapes<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Vec<(&'static str, String)> {
    entries
        .into_iter()
        .map(|e| match e {
            Entry::User { text, .. } => ("user", text.clone()),
            Entry::Agent { text, .. } => ("agent", text.clone()),
            Entry::Thought { text, .. } => ("thought", text.clone()),
            Entry::Notice { text, .. } => ("notice", text.clone()),
            Entry::Tool { title, .. } => ("tool", title.clone()),
        })
        .collect()
}

fn followed(office: &Office) -> Vec<String> {
    lock(&office.chats).open.keys().cloned().collect()
}

fn prompt_card(detail: &str) -> Approval {
    Approval {
        id: "terminal-prompt".to_string(),
        title: "Claude Code is waiting for your answer".to_string(),
        detail: detail.to_string(),
        fields: Vec::new(),
        choices: Vec::new(),
    }
}

#[test]
fn an_app_terminal_desk_gets_its_transcript_with_the_terminal_capabilities() {
    let (office, dir, floor) = floor_office("follow-backlog");
    let (id, _) = desk(&office, &dir, &floor, &[PROMPT, SAY]);
    let backlog = office.chat_backlog(&id).unwrap();
    assert_eq!(
        shapes(&backlog.transcript),
        [
            ("user", "fix the parser".to_string()),
            ("agent", "Running the tests first.".to_string())
        ]
    );
    assert_eq!(backlog.capabilities, Capabilities::CLAUDE_TERMINAL);
    assert_eq!(backlog.state, SessionState::Idle);
    assert_eq!((backlog.approval, backlog.ended), (None, None));
    assert!(!backlog.earlier);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn appended_lines_arrive_once_each_with_the_sessions_state() {
    let (office, dir, floor) = floor_office("follow-append");
    let (id, path) = desk(&office, &dir, &floor, &[PROMPT, SAY]);
    office.chat_backlog(&id).unwrap();
    append(&path, &[BASH, DONE]);
    let updates = office.chat_updates();
    assert_eq!(
        shapes(updates.iter().filter_map(|u| u.entry.as_ref())),
        [
            ("tool", "pnpm test".to_string()),
            ("agent", "Fixed: the last token is kept.".to_string())
        ]
    );
    assert_eq!(updates.len(), 2);
    assert!(updates.iter().all(|u| u.session_id == id
        && u.state == SessionState::Idle
        && !u.reset
        && !u.desk_changed));
    assert!(
        office.chat_updates().is_empty(),
        "nothing new, nothing sent"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_state_change_alone_sends_one_update_with_no_entry() {
    let (office, dir, floor) = floor_office("follow-state");
    let (id, _) = desk(&office, &dir, &floor, &[PROMPT]);
    office.chat_backlog(&id).unwrap();
    event(&office, &id, "PreToolUse", None);
    apply_feed(&office);
    let updates = office.chat_updates();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].entry, None);
    assert_eq!(updates[0].state, SessionState::Running);
    assert!(office.chat_updates().is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_permission_prompt_shows_as_a_terminal_card_until_the_tool_runs() {
    let (office, dir, floor) = floor_office("follow-prompt");
    let (id, _) = desk(&office, &dir, &floor, &[PROMPT]);
    office.chat_backlog(&id).unwrap();
    notify(
        &office,
        &id,
        "permission_prompt",
        "The agent wants to run a command",
    );
    let card = prompt_card("The agent wants to run a command");
    let updates = office.chat_updates();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].approval.as_ref(), Some(&card));
    assert_eq!(updates[0].state, SessionState::WaitingHuman);
    assert_eq!(office.chat_backlog(&id).unwrap().approval, Some(card));
    event(&office, &id, "PostToolUse", None);
    apply_feed(&office);
    let updates = office.chat_updates();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].approval, None);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_prompts_message_arrives_cleaned_on_one_line() {
    let (office, dir, floor) = floor_office("follow-clean");
    let (id, _) = desk(&office, &dir, &floor, &[PROMPT]);
    notify(
        &office,
        &id,
        "permission_prompt",
        "Run\u{1b}[31m this\ncommand?",
    );
    let backlog = office.chat_backlog(&id).unwrap();
    assert_eq!(backlog.approval, Some(prompt_card("Run this command?")));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_hand_started_session_is_read_only() {
    let (office, dir, floor) = floor_office("follow-hand");
    let id = "hand-started-1";
    office.lock().found.sessions.push(session(
        id,
        &floor,
        ToolKind::ClaudeCode,
        ControlMode::RaiseWindow,
    ));
    append(&transcript(&dir, id), &[PROMPT]);
    let backlog = office.chat_backlog(id).unwrap();
    assert_eq!(backlog.capabilities, Capabilities::READ_ONLY);
    assert_eq!(backlog.transcript.len(), 1);
    fs::remove_dir_all(&dir).unwrap();
}

/// A structured desk's driver that does nothing.
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

#[test]
fn a_structured_desk_answers_from_its_own_backlog_and_is_not_followed() {
    let (office, dir, floor) = floor_office("follow-structured");
    let info = DeskInfo {
        tool: ToolKind::Codex,
        project_id: floor.id.clone(),
        root: dir.join("home/code/app"),
        title: "structured".to_string(),
        model: "default".to_string(),
        effort: "default".to_string(),
    };
    office
        .structured
        .seat("structured-1", info, SessionState::Idle, Box::new(Quiet));
    let backlog = office.chat_backlog("structured-1").unwrap();
    assert_eq!(backlog.capabilities, Capabilities::STRUCTURED);
    assert!(followed(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_codex_terminal_and_an_unknown_id_have_no_conversation() {
    let (office, dir, floor) = floor_office("follow-none");
    let (_, codex) = seat(&office, &dir, &floor, ToolKind::Codex, SessionState::Idle);
    let refused = Err("that desk has no conversation to show".to_string());
    assert_eq!(office.chat_backlog(&codex).map(|_| ()), refused);
    assert_eq!(office.chat_backlog("nobody-1").map(|_| ()), refused);
    assert!(followed(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_desk_with_no_transcript_yet_starts_empty_and_fills_once_written() {
    let (office, dir, floor) = floor_office("follow-later");
    let (id, path) = desk(&office, &dir, &floor, &[]);
    let backlog = office.chat_backlog(&id).unwrap();
    assert!(backlog.transcript.is_empty());
    assert!(office.chat_updates().is_empty());
    append(&path, &[PROMPT, SAY]);
    let updates = office.chat_updates();
    assert_eq!(updates.len(), 2);
    assert!(updates.iter().all(|u| u.entry.is_some() && !u.reset));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn keep_chat_drops_every_other_follower() {
    let (office, dir, floor) = floor_office("follow-keep");
    let (id, path) = desk(&office, &dir, &floor, &[PROMPT]);
    office.chat_backlog(&id).unwrap();
    office.keep_chat(office.chat_ticket(), Some(&id));
    assert_eq!(followed(&office), std::slice::from_ref(&id));
    office.keep_chat(office.chat_ticket(), Some("other-1"));
    append(&path, &[SAY]);
    assert!(office.chat_updates().is_empty());
    office.chat_backlog(&id).unwrap();
    office.keep_chat(office.chat_ticket(), None);
    assert!(followed(&office).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_stale_keep_chat_does_not_drop_the_newer_desk() {
    let (office, dir, floor) = floor_office("follow-stale");
    let (id, _) = desk(&office, &dir, &floor, &[PROMPT]);
    let stale = office.chat_ticket();
    let fresh = office.chat_ticket();
    office.keep_chat(fresh, Some(&id));
    office.chat_backlog(&id).unwrap();
    office.keep_chat(stale, Some("other-1"));
    assert_eq!(followed(&office), std::slice::from_ref(&id));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_new_session_in_the_terminal_ends_the_old_conversation() {
    let (office, dir, floor) = floor_office("follow-cleared");
    let (id, _) = desk(&office, &dir, &floor, &[PROMPT]);
    office.chat_backlog(&id).unwrap();
    {
        let mut inner = office.lock();
        let terminal = inner.ran_in[&id].clone();
        inner.owned.insert(terminal, "s-after-clear".to_string());
        inner.found.sessions.retain(|s| s.id != id);
    }
    let updates = office.chat_updates();
    assert_eq!(updates.len(), 1);
    assert_eq!(
        updates[0].ended.as_deref(),
        Some("A new conversation was started in this terminal. Open the desk again to follow it.")
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_transcript_rewritten_shorter_resets_the_conversation() {
    let (office, dir, floor) = floor_office("follow-reset");
    let (id, path) = desk(&office, &dir, &floor, &[PROMPT, SAY, BASH]);
    office.chat_backlog(&id).unwrap();
    fs::write(&path, text(&[DONE])).unwrap();
    let updates = office.chat_updates();
    assert!(updates[0].reset);
    assert_eq!(
        shapes(updates.iter().filter_map(|u| u.entry.as_ref())),
        [("agent", "Fixed: the last token is kept.".to_string())]
    );
    assert!(updates[1..].iter().all(|u| !u.reset));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reading_a_long_transcript_does_not_hold_the_office_lock() {
    let (office, dir, floor) = floor_office("follow-long");
    let (id, path) = desk(&office, &dir, &floor, &[]);
    let filler = format!(r#"{{"type":"summary","summary":"{}"}}"#, "x".repeat(1000));
    let mut body = String::with_capacity(10 * 1024 * 1024);
    while body.len() < 9 * 1024 * 1024 {
        body.push_str(&filler);
        body.push('\n');
    }
    body.push_str(&text(&[PROMPT, SAY]));
    fs::write(&path, body).unwrap();
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| office.chat_backlog(&id));
        let snapshot = scope.spawn(|| office.snapshot());
        assert!(!snapshot.join().unwrap().projects.is_empty());
        let backlog = reader.join().unwrap().unwrap();
        assert_eq!(backlog.transcript.len(), 2);
        assert!(backlog.earlier);
    });
    fs::remove_dir_all(&dir).unwrap();
}
