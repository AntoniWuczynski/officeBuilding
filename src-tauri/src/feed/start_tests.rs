//! The start mark (stage M1, rule A2) and the notification types that say nothing (rule A5),
//! driven through event files whose modification time each test sets.

use super::*;
use std::fs::File;
use std::sync::atomic::{AtomicUsize, Ordering};

fn feed(tag: &str) -> Feed {
    let dir = std::env::temp_dir().join(format!("ob-feed-start-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    Feed::new(dir)
}

/// Drop `json` as an event file of `event` dated `at`, then collect it at `at`.
fn drop_at(feed: &mut Feed, event: HookEvent, json: &str, at: SystemTime) {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = feed.dir().join(format!("{}.{n:06}", event.name()));
    fs::write(&path, json).expect("write");
    File::options()
        .write(true)
        .open(&path)
        .and_then(|f| f.set_modified(at))
        .expect("set the time");
    feed.collect(at);
}

fn start(feed: &mut Feed, source: &str, at: SystemTime) {
    let json = format!(r#"{{"session_id":"s1","source":"{source}"}}"#);
    drop_at(feed, HookEvent::SessionStart, &json, at);
}

fn prompt(feed: &mut Feed, at: SystemTime) {
    drop_at(
        feed,
        HookEvent::UserPromptSubmit,
        r#"{"session_id":"s1","prompt":"go"}"#,
        at,
    );
}

fn state(feed: &Feed) -> Option<SessionState> {
    feed.seen("s1").map(|(s, _)| s)
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn a_startup_mark_alone_reads_idle_and_settles_for_ten_seconds() {
    let mut feed = feed("alone");
    let at = SystemTime::now() - secs(60);
    start(&mut feed, "startup", at);
    assert_eq!(feed.seen("s1"), Some((SessionState::Idle, at)));
    assert!(feed.reports("s1"));
    assert!(feed.settling("s1", at));
    assert!(feed.settling("s1", at + secs(9)));
    assert!(!feed.settling("s1", at + secs(10)));
    assert!(!feed.settling("s2", at));
}

#[test]
fn a_prompt_after_the_mark_reads_thinking() {
    let mut feed = feed("after");
    let at = SystemTime::now() - secs(60);
    start(&mut feed, "startup", at);
    prompt(&mut feed, at + secs(1));
    assert_eq!(state(&feed), Some(SessionState::Thinking));
    assert!(!feed.settling("s1", at + secs(1)));
}

#[test]
fn a_prompt_up_to_ten_seconds_before_the_mark_wins_in_either_order() {
    let at = SystemTime::now() - secs(60);
    let before = at - secs(2);
    let mut mark_first = feed("mark-first");
    start(&mut mark_first, "startup", at);
    prompt(&mut mark_first, before);
    let mut prompt_first = feed("prompt-first");
    prompt(&mut prompt_first, before);
    start(&mut prompt_first, "startup", at);
    for feed in [&mark_first, &prompt_first] {
        assert_eq!(feed.seen("s1"), Some((SessionState::Thinking, before)));
        assert!(!feed.settling("s1", at));
    }
}

#[test]
fn a_real_event_eleven_seconds_older_than_a_resume_mark_loses() {
    let at = SystemTime::now() - secs(60);
    let older = at - secs(11);
    let mut mark_first = feed("resume-mark-first");
    start(&mut mark_first, "resume", at);
    prompt(&mut mark_first, older);
    let mut prompt_first = feed("resume-prompt-first");
    prompt(&mut prompt_first, older);
    start(&mut prompt_first, "resume", at);
    for feed in [&mark_first, &prompt_first] {
        assert_eq!(feed.seen("s1"), Some((SessionState::Idle, at)));
    }
}

#[test]
fn only_startup_resume_and_clear_make_a_mark() {
    let rows: [(Option<&str>, bool); 7] = [
        (Some("startup"), true),
        (Some("resume"), true),
        (Some("clear"), true),
        (Some("compact"), false),
        (Some("fork"), false),
        (Some("teleport"), false),
        (None, false),
    ];
    for (i, (source, marks)) in rows.into_iter().enumerate() {
        let mut feed = feed(&format!("source-{i}"));
        let json = source.map_or_else(
            || r#"{"session_id":"s1"}"#.to_string(),
            |s| format!(r#"{{"session_id":"s1","source":"{s}"}}"#),
        );
        drop_at(
            &mut feed,
            HookEvent::SessionStart,
            &json,
            SystemTime::now() - secs(60),
        );
        let want = marks.then_some(SessionState::Idle);
        assert_eq!(state(&feed), want, "{source:?}");
    }
}

#[test]
fn six_notification_types_leave_the_state_alone_and_an_unknown_one_waits() {
    let rows: [(&str, SessionState); 7] = [
        ("auth_success", SessionState::Running),
        ("auth_storage_failure", SessionState::Running),
        ("elicitation_complete", SessionState::Running),
        ("elicitation_response", SessionState::Running),
        ("agent_completed", SessionState::Running),
        ("plugin_notification", SessionState::Running),
        ("something_new", SessionState::WaitingHuman),
    ];
    for (kind, want) in rows {
        let mut feed = feed(&format!("note-{kind}"));
        let at = SystemTime::now() - secs(60);
        drop_at(
            &mut feed,
            HookEvent::PreToolUse,
            r#"{"session_id":"s1","tool_name":"Bash"}"#,
            at,
        );
        let json = format!(r#"{{"session_id":"s1","notification_type":"{kind}","message":"m"}}"#);
        drop_at(&mut feed, HookEvent::Notification, &json, at + secs(1));
        assert_eq!(state(&feed), Some(want), "{kind}");
    }
}
