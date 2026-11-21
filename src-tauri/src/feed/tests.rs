use super::*;
use crate::handover::{Task, TaskKind};
use crate::model::{ControlMode, Spend};
use std::io::Write;
use std::process::{Command, Stdio};

fn feed(tag: &str) -> Feed {
    let dir = std::env::temp_dir().join(format!("ob-feed-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    Feed::new(dir)
}

/// Run the hook command the way a tool would: through `sh`, with the event on stdin.
fn fire(feed: &Feed, event: HookEvent, json: &str) {
    let mut child = Command::new("/bin/sh")
        .args(["-c", &feed.command(event)])
        .stdin(Stdio::piped())
        .spawn()
        .expect("sh");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(json.as_bytes())
        .expect("write");
    assert!(child.wait().expect("wait").success());
}

fn session(id: &str, last: &str) -> Session {
    Session {
        session_name: None,
        session_name_source: None,
        self_reviewed: false,
        id: id.to_string(),
        tool: ToolKind::ClaudeCode,
        project_id: "p".to_string(),
        title: "t".to_string(),
        state: SessionState::Running,
        control: ControlMode::Full,
        model: None,
        effort: None,
        last_reply: None,
        last_activity_at: last.to_string(),
        pending_question_ids: Vec::new(),
        spend: Spend::default(),
        helpers: Vec::new(),
        pets: Vec::new(),
        worktree: false,
        branch: None,
    }
}

#[test]
fn a_permission_prompt_raises_the_agents_hand_until_it_moves_on() {
    let mut feed = feed("prompt");
    fire(
        &feed,
        HookEvent::Notification,
        r#"{"session_id":"s1","notification_type":"permission_prompt","message":"Claude needs your permission"}"#,
    );
    feed.collect(SystemTime::now());
    assert_eq!(
        fs::read_dir(feed.dir()).expect("dir").count(),
        0,
        "files are taken"
    );
    let mut sessions = vec![
        session("s1", "2000-01-01T00:00:00Z"),
        session("s2", "2000-01-01T00:00:00Z"),
    ];
    feed.apply(&mut sessions);
    assert_eq!(sessions[0].state, SessionState::WaitingHuman);
    assert_eq!(
        sessions[1].state,
        SessionState::Running,
        "no events: unchanged"
    );
    fire(
        &feed,
        HookEvent::PreToolUse,
        r#"{"session_id":"s1","tool_name":"Bash"}"#,
    );
    feed.collect(SystemTime::now());
    feed.apply(&mut sessions);
    assert_eq!(sessions[0].state, SessionState::Running);
}

#[test]
fn a_session_whose_files_are_newer_keeps_their_state() {
    let mut feed = feed("older");
    fire(&feed, HookEvent::Stop, r#"{"session_id":"s1"}"#);
    feed.collect(SystemTime::now());
    let mut sessions = vec![session("s1", "2999-01-01T00:00:00Z")];
    feed.apply(&mut sessions);
    assert_eq!(sessions[0].state, SessionState::Running);
}

#[test]
fn a_file_still_being_written_is_left_for_the_next_look() {
    let mut feed = feed("partial");
    fs::create_dir_all(feed.dir()).expect("mkdir");
    fs::write(feed.dir().join("Stop.abc123"), r#"{"session_"#).expect("write");
    fs::write(feed.dir().join("notes.txt"), "not an event").expect("write");
    feed.collect(SystemTime::now());
    assert!(feed.dir().join("Stop.abc123").exists());
    assert!(!feed.dir().join("notes.txt").exists());
    feed.collect(SystemTime::now() + WRITE_GRACE + Duration::from_secs(1));
    assert!(
        !feed.dir().join("Stop.abc123").exists(),
        "dropped once it can no longer be in flight"
    );
}

// Codex fires `PermissionRequest` just before its approval prompt. Since spec 5.5 it is a
// request the feed keeps, not a state: the reconcile raises the hand from it
// (`attention/tests.rs`), so the feed leaves the session's state alone.
#[test]
fn a_codex_approval_prompt_is_a_request_and_not_a_state() {
    assert!(events(ToolKind::Codex).contains(&HookEvent::PermissionRequest));
    let mut feed = feed("codex-approval");
    fire(
        &feed,
        HookEvent::PermissionRequest,
        r#"{"session_id":"c1","tool_name":"Bash","tool_input":{"command":"tool-x --flag","description":null}}"#,
    );
    feed.collect(SystemTime::now() + Duration::from_millis(10));
    let mut sessions = vec![session("c1", "2000-01-01T00:00:00Z")];
    sessions[0].tool = ToolKind::Codex;
    feed.apply(&mut sessions);
    assert_eq!(sessions[0].state, SessionState::Running);
    assert_eq!(feed.seen("c1"), None);
    assert_eq!(feed.requests().open("c1").len(), 1);
}

#[test]
fn reports_only_sessions_it_has_an_event_for() {
    let mut feed = feed("reports");
    assert!(!feed.reports("s1"));
    fire(&feed, HookEvent::Stop, r#"{"session_id":"s1"}"#);
    feed.collect(SystemTime::now());
    assert!(feed.reports("s1"));
    assert!(!feed.reports("s2"));
}

#[test]
fn seen_keeps_the_newest_event_past_the_stale_limit() {
    let mut feed = feed("seen");
    assert_eq!(feed.seen("s1"), None);
    fire(&feed, HookEvent::PreToolUse, r#"{"session_id":"s1"}"#);
    feed.collect(SystemTime::now());
    assert_eq!(feed.seen("s1").map(|(s, _)| s), Some(SessionState::Running));
    let before = SystemTime::now();
    fire(&feed, HookEvent::Stop, r#"{"session_id":"s1"}"#);
    feed.collect(SystemTime::now());
    let (state, at) = feed.seen("s1").expect("seen");
    assert_eq!(state, SessionState::Idle);
    assert!(
        at + Duration::from_secs(2) >= before,
        "the event's own time"
    );
    feed.collect(at + STALE + Duration::from_secs(1));
    assert!(!feed.reports("s1"), "stale for the state");
    assert_eq!(feed.seen("s1"), Some((SessionState::Idle, at)));
}

#[test]
fn a_permission_prompts_message_is_kept_cleaned_until_the_next_event() {
    let mut feed = feed("asked");
    fire(
        &feed,
        HookEvent::Notification,
        r#"{"session_id":"s1","notification_type":"permission_prompt","message":"Run\u001b[31m this\ncommand?"}"#,
    );
    feed.collect(SystemTime::now());
    assert_eq!(feed.asked("s1"), Some("Run this command?"));
    assert_eq!(feed.asked("s2"), None);
    std::thread::sleep(Duration::from_millis(15));
    fire(&feed, HookEvent::PostToolUse, r#"{"session_id":"s1"}"#);
    feed.collect(SystemTime::now());
    assert_eq!(feed.asked("s1"), None, "forgotten at the next event");
}

#[test]
fn an_idle_prompt_keeps_no_message() {
    let mut feed = feed("idle-asked");
    fire(
        &feed,
        HookEvent::Notification,
        r#"{"session_id":"s1","notification_type":"idle_prompt","message":"Claude is waiting for your input"}"#,
    );
    feed.collect(SystemTime::now());
    assert_eq!(feed.asked("s1"), None);
}

#[test]
fn codex_gets_no_notification_hook() {
    assert!(!events(ToolKind::Codex).contains(&HookEvent::Notification));
    assert!(events(ToolKind::ClaudeCode).contains(&HookEvent::Notification));
    assert!(events(ToolKind::Opencode).is_empty());
}

#[test]
fn claude_code_reports_the_handover_events_and_codex_is_unchanged() {
    let claude = events(ToolKind::ClaudeCode);
    for event in [
        HookEvent::SessionStart,
        HookEvent::SubagentStop,
        HookEvent::StopFailure,
    ] {
        assert!(claude.contains(&event), "{}", event.name());
    }
    assert_eq!(
        events(ToolKind::Codex),
        &[
            HookEvent::UserPromptSubmit,
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::PermissionRequest,
            HookEvent::Stop,
        ]
    );
}

#[test]
fn the_state_table() {
    let payload = |notification: Option<&str>| Payload {
        session_id: "s1".into(),
        notification_type: notification.map(String::from),
        message: None,
        source: None,
    };
    let rows: [(HookEvent, Option<&str>, Option<SessionState>); 13] = [
        (
            HookEvent::UserPromptSubmit,
            None,
            Some(SessionState::Thinking),
        ),
        (HookEvent::PostToolUse, None, Some(SessionState::Thinking)),
        (HookEvent::PreToolUse, None, Some(SessionState::Running)),
        (
            HookEvent::Notification,
            Some("idle_prompt"),
            Some(SessionState::Idle),
        ),
        (
            HookEvent::Notification,
            Some("quota_auto_resume_disabled"),
            Some(SessionState::Idle),
        ),
        (
            HookEvent::Notification,
            Some("quota_auto_resume_fired"),
            Some(SessionState::Thinking),
        ),
        (
            HookEvent::Notification,
            Some("quota_auto_resume_stale"),
            Some(SessionState::WaitingHuman),
        ),
        (
            HookEvent::Notification,
            Some("permission_prompt"),
            Some(SessionState::WaitingHuman),
        ),
        // Spec 5.5: a permission request is kept as a request, not a state.
        (HookEvent::PermissionRequest, None, None),
        (HookEvent::Stop, None, Some(SessionState::Idle)),
        (HookEvent::StopFailure, None, Some(SessionState::Error)),
        (HookEvent::SessionStart, None, None),
        (HookEvent::SubagentStop, None, None),
    ];
    for (event, notification, want) in rows {
        assert_eq!(
            state_of(event, &payload(notification)),
            want,
            "{} {notification:?}",
            event.name()
        );
    }
}

#[test]
fn a_session_start_or_subagent_stop_leaves_the_desk_alone() {
    let mut feed = feed("no-state");
    fire(
        &feed,
        HookEvent::Notification,
        r#"{"session_id":"s1","notification_type":"permission_prompt","message":"May I?"}"#,
    );
    feed.collect(SystemTime::now());
    let before = feed.seen("s1");
    std::thread::sleep(Duration::from_millis(15));
    fire(
        &feed,
        HookEvent::SessionStart,
        r#"{"session_id":"s1","source":"startup"}"#,
    );
    fire(
        &feed,
        HookEvent::SubagentStop,
        r#"{"session_id":"s1","background_tasks":[],"session_crons":[]}"#,
    );
    feed.collect(SystemTime::now());
    assert_eq!(fs::read_dir(feed.dir()).expect("dir").count(), 0, "taken");
    assert_eq!(feed.seen("s1"), before);
    assert_eq!(feed.asked("s1"), Some("May I?"));
    fire(
        &feed,
        HookEvent::SessionStart,
        r#"{"session_id":"s2","source":"startup"}"#,
    );
    feed.collect(SystemTime::now());
    // A start alone is a start mark now (stage M1, rule A2): idle, and settling at first.
    assert_eq!(feed.seen("s2").map(|(s, _)| s), Some(SessionState::Idle));
    assert!(feed.settling("s2", SystemTime::now()));
    assert_eq!(feed.take_signals().len(), 3);
}

#[test]
fn a_stop_failure_puts_the_desk_in_error() {
    let mut feed = feed("stop-failure");
    fire(
        &feed,
        HookEvent::StopFailure,
        r#"{"session_id":"s1","error":"rate_limit","last_assistant_message":"API Error"}"#,
    );
    feed.collect(SystemTime::now());
    assert_eq!(feed.seen("s1").map(|(s, _)| s), Some(SessionState::Error));
    assert!(feed.take_signals().is_empty(), "a wake-up, never a signal");
}

/// A signal without its time, to compare.
#[derive(Debug, PartialEq)]
enum Said {
    Listed(Option<Vec<Task>>),
    Started(TaskKind),
}

fn said(signal: Signal) -> (String, Said) {
    match signal {
        Signal::Listed {
            session_id, tasks, ..
        } => (session_id, Said::Listed(tasks)),
        Signal::Started {
            session_id, task, ..
        } => (session_id, Said::Started(task.kind)),
    }
}

/// Fire one event through `sh` into a fresh feed: its signals and the state it set.
fn signals_of(tag: &str, event: HookEvent, json: &str) -> (Vec<Said>, Option<SessionState>) {
    let mut feed = feed(tag);
    fire(&feed, event, json);
    feed.collect(SystemTime::now());
    assert_eq!(
        fs::read_dir(feed.dir()).expect("dir").count(),
        0,
        "{tag}: taken"
    );
    let signals = feed
        .take_signals()
        .into_iter()
        .map(|s| {
            let (id, said) = said(s);
            assert_eq!(id, "s1");
            said
        })
        .collect();
    (signals, feed.seen("s1").map(|(s, _)| s))
}

fn task(kind: TaskKind, name: Option<&str>) -> Task {
    Task {
        kind,
        name: name.map(String::from),
    }
}

#[test]
fn the_signal_table() {
    let rows: Vec<(HookEvent, &str, Vec<Said>)> = vec![
        (
            HookEvent::SessionStart,
            r#"{"session_id":"s1","source":"startup"}"#,
            vec![Said::Listed(Some(vec![]))],
        ),
        (
            HookEvent::SessionStart,
            r#"{"session_id":"s1","source":"resume"}"#,
            vec![],
        ),
        (HookEvent::SessionStart, r#"{"session_id":"s1"}"#, vec![]),
        (
            HookEvent::Stop,
            r#"{"session_id":"s1","background_tasks":[],"session_crons":[]}"#,
            vec![Said::Listed(Some(vec![]))],
        ),
        (
            HookEvent::Stop,
            r#"{"session_id":"s1","background_tasks":[
                {"id":"t1","type":"workflow","status":"running","name":"release"},
                {"id":"t2","type":"shell","status":"running","command":"tail -f log"},
                {"id":"t3","type":"subagent","status":"running","agent_type":"x"},
                {"id":"t4","type":"MCP task","status":"running"}],
               "session_crons":[{"id":"c1","schedule":"0 9 * * 1-5","recurring":true,"prompt":"p"}]}"#,
            vec![Said::Listed(Some(vec![
                task(TaskKind::Workflow, Some("release")),
                task(TaskKind::Shell, None),
                task(TaskKind::Subagent, None),
                task(TaskKind::Other, None),
                task(TaskKind::Scheduled, None),
            ]))],
        ),
        (
            HookEvent::SubagentStop,
            r#"{"session_id":"s1","agent_id":"a1","background_tasks":[{"type":"workflow"}],"session_crons":[]}"#,
            vec![Said::Listed(Some(vec![task(TaskKind::Workflow, None)]))],
        ),
        (
            HookEvent::Stop,
            r#"{"session_id":"s1","background_tasks":[]}"#,
            vec![Said::Listed(None)],
        ),
        (
            HookEvent::SubagentStop,
            r#"{"session_id":"s1","session_crons":[]}"#,
            vec![Said::Listed(None)],
        ),
        (
            HookEvent::PreToolUse,
            r#"{"session_id":"s1","tool_name":"Workflow","tool_input":{}}"#,
            vec![Said::Started(TaskKind::Workflow)],
        ),
        (
            HookEvent::PreToolUse,
            r#"{"session_id":"s1","tool_name":"Bash","tool_input":{"command":"sleep 9","run_in_background":true}}"#,
            vec![Said::Started(TaskKind::Shell)],
        ),
        (
            HookEvent::PreToolUse,
            r#"{"session_id":"s1","tool_name":"Bash","tool_input":{"command":"ls","run_in_background":false}}"#,
            vec![],
        ),
        (
            HookEvent::PreToolUse,
            r#"{"session_id":"s1","tool_name":"Bash","tool_input":{"command":"ls"}}"#,
            vec![],
        ),
        (
            HookEvent::PostToolUse,
            r#"{"session_id":"s1","tool_name":"Agent","tool_response":{"status":"async_launched","agentId":"a1"}}"#,
            vec![Said::Started(TaskKind::Subagent)],
        ),
        (
            HookEvent::PostToolUse,
            r#"{"session_id":"s1","tool_name":"Agent","tool_response":{"status":"completed"}}"#,
            vec![],
        ),
        (
            HookEvent::PreToolUse,
            r#"{"session_id":"s1","tool_name":"Agent","tool_input":{"run_in_background":true}}"#,
            vec![Said::Started(TaskKind::Subagent)],
        ),
        (
            HookEvent::UserPromptSubmit,
            r#"{"session_id":"s1","prompt":"go"}"#,
            vec![],
        ),
        (
            HookEvent::StopFailure,
            r#"{"session_id":"s1","error":"rate_limit"}"#,
            vec![],
        ),
    ];
    for (i, (event, json, want)) in rows.into_iter().enumerate() {
        let (got, _) = signals_of(&format!("signal-{i}"), event, json);
        assert_eq!(got, want, "row {i}: {} {json}", event.name());
    }
}

#[test]
fn only_a_tool_that_starts_work_outliving_the_turn_counts() {
    let rows = [
        (
            r#"{"session_id":"s1","tool_name":"Monitor","tool_input":{}}"#,
            vec![Said::Started(TaskKind::Other)],
        ),
        (
            r#"{"session_id":"s1","tool_name":"CronCreate","tool_input":{}}"#,
            vec![Said::Started(TaskKind::Scheduled)],
        ),
        (
            r#"{"session_id":"s1","tool_name":"ScheduleWakeup","tool_input":{}}"#,
            vec![Said::Started(TaskKind::Scheduled)],
        ),
        (
            r#"{"session_id":"s1","tool_name":"Read","tool_input":{}}"#,
            vec![],
        ),
        (
            r#"{"session_id":"s1","tool_name":"mcp__server__tool","tool_input":{}}"#,
            vec![],
        ),
        (
            r#"{"session_id":"s1","tool_name":"Agent","tool_input":{"prompt":"x"}}"#,
            vec![],
        ),
    ];
    for (i, (json, want)) in rows.into_iter().enumerate() {
        let (got, _) = signals_of(&format!("inert-{i}"), HookEvent::PreToolUse, json);
        assert_eq!(got, want, "row {i}: {json}");
    }
}

#[test]
fn fields_of_the_wrong_shape_never_drop_the_event() {
    let rows: [(HookEvent, &str, Vec<Said>, Option<SessionState>); 5] = [
        (
            HookEvent::Stop,
            r#"{"session_id":"s1","background_tasks":"x","session_crons":[]}"#,
            vec![Said::Listed(None)],
            Some(SessionState::Idle),
        ),
        (
            HookEvent::Stop,
            r#"{"session_id":"s1","background_tasks":[],"session_crons":{}}"#,
            vec![Said::Listed(None)],
            Some(SessionState::Idle),
        ),
        (
            HookEvent::Stop,
            r#"{"session_id":"s1","background_tasks":[{"name":"no type"}],"session_crons":[]}"#,
            vec![Said::Listed(None)],
            Some(SessionState::Idle),
        ),
        (
            HookEvent::PostToolUse,
            r#"{"session_id":"s1","tool_name":"Agent","tool_response":"text"}"#,
            vec![],
            Some(SessionState::Thinking),
        ),
        (
            HookEvent::PreToolUse,
            r#"{"session_id":"s1","tool_name":"Bash","tool_input":3}"#,
            vec![],
            Some(SessionState::Running),
        ),
    ];
    for (i, (event, json, want, state)) in rows.into_iter().enumerate() {
        let (got, seen) = signals_of(&format!("shape-{i}"), event, json);
        assert_eq!(got, want, "row {i}: {json}");
        assert_eq!(seen, state, "row {i}: {json}");
    }
}

#[test]
fn take_signals_is_oldest_first_and_empties_the_list() {
    let mut feed = feed("take");
    fire(
        &feed,
        HookEvent::PreToolUse,
        r#"{"session_id":"s1","tool_name":"Workflow"}"#,
    );
    std::thread::sleep(Duration::from_millis(15));
    fire(
        &feed,
        HookEvent::Stop,
        r#"{"session_id":"s1","background_tasks":[],"session_crons":[]}"#,
    );
    feed.collect(SystemTime::now());
    let taken = feed.take_signals();
    assert_eq!(taken.len(), 2);
    assert!(signals::at(&taken[0]) < signals::at(&taken[1]));
    assert_eq!(said(taken[0].clone()).1, Said::Started(TaskKind::Workflow));
    assert!(feed.take_signals().is_empty());
}

#[test]
fn a_task_name_comes_out_clean() {
    let (got, _) = signals_of(
        "clean-name",
        HookEvent::Stop,
        r#"{"session_id":"s1","background_tasks":[{"type":"workflow","name":"rel\u001b[31mease\nnow"}],"session_crons":[]}"#,
    );
    assert_eq!(
        got,
        vec![Said::Listed(Some(vec![task(
            TaskKind::Workflow,
            Some("release now")
        )]))]
    );
}
