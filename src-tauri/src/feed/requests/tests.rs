//! Hook files composed from F19 (Src 161 `hooks/src/schema.rs`), not replayed: placeholders
//! only, written with explicit times.

use super::*;
use crate::feed::Feed;
use std::fs::{self, File};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const S: &str = "s-0001";

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn base() -> SystemTime {
    SystemTime::UNIX_EPOCH + secs(1_800_000_000)
}

fn feed(tag: &str) -> Feed {
    let dir = std::env::temp_dir().join(format!("ob-reqs-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    Feed::new(dir)
}

/// A hook's input. `extra` is raw JSON members after the session id.
fn body(session: &str, extra: &str) -> String {
    format!(r#"{{"session_id":"{session}"{extra}}}"#)
}

fn call(session: &str, turn: &str, id: &str, cmd: &str) -> String {
    body(
        session,
        &format!(
            r#","turn_id":"{turn}","tool_name":"shell","tool_use_id":"{id}","tool_input":{{"command":"{cmd}"}}"#
        ),
    )
}

fn ask(session: &str, turn: &str, cmd: &str) -> String {
    body(
        session,
        &format!(r#","turn_id":"{turn}","tool_name":"shell","tool_input":{{"command":"{cmd}"}}"#),
    )
}

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

fn open(feed: &Feed, session: &str) -> usize {
    feed.requests().open(session).len()
}

#[test]
fn a_request_is_tied_to_the_call_it_is_about() {
    let mut f = feed("tied");
    drop_at(
        &mut f,
        HookEvent::PreToolUse,
        &call(S, "t1", "c1", "tool-x --flag"),
        base(),
    );
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &ask(S, "t1", "tool-x --flag"),
        base() + secs(1),
    );
    let [asked] = f.requests().open(S) else {
        panic!("one open request");
    };
    assert_eq!(asked.call_id.as_deref(), Some("c1"));
    assert_eq!(asked.tool, "shell");
    assert_eq!(asked.detail, "tool-x --flag");
    assert_eq!(asked.turn_id.as_deref(), Some("t1"));
}

#[test]
fn another_call_finishing_and_time_do_not_retire_it_but_its_own_post_does() {
    let mut f = feed("post");
    drop_at(
        &mut f,
        HookEvent::PreToolUse,
        &call(S, "t1", "c1", "tool-x"),
        base(),
    );
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &ask(S, "t1", "tool-x"),
        base() + secs(1),
    );
    drop_at(
        &mut f,
        HookEvent::PostToolUse,
        &call(S, "t1", "c2", "tool-y"),
        base() + secs(2),
    );
    assert_eq!(open(&f, S), 1);
    f.collect(base() + secs(31 * 60 + 2));
    assert_eq!(open(&f, S), 1, "31 minutes later");
    drop_at(
        &mut f,
        HookEvent::PostToolUse,
        &call(S, "t1", "c1", "tool-x"),
        base() + secs(3),
    );
    assert_eq!(open(&f, S), 0);
}

#[test]
fn a_request_with_no_call_is_retired_by_the_same_tool_and_input() {
    let mut f = feed("match");
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &ask(S, "t1", "tool-x"),
        base(),
    );
    assert_eq!(f.requests().open(S)[0].call_id, None);
    drop_at(
        &mut f,
        HookEvent::PostToolUse,
        &ask(S, "t1", "tool-y"),
        base() + secs(1),
    );
    assert_eq!(open(&f, S), 1, "another input");
    drop_at(
        &mut f,
        HookEvent::PostToolUse,
        &ask(S, "t1", "tool-x"),
        base() + secs(2),
    );
    assert_eq!(open(&f, S), 0);
}

#[test]
fn a_stop_retires_it() {
    let mut f = feed("stop");
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &ask(S, "t1", "tool-x"),
        base(),
    );
    drop_at(
        &mut f,
        HookEvent::Stop,
        &body(S, r#","turn_id":"t1""#),
        base() + secs(1),
    );
    assert_eq!(open(&f, S), 0);
}

#[test]
fn a_steer_leaves_it_open_and_a_root_event_of_another_turn_retires_it() {
    let mut f = feed("steer");
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &ask(S, "t1", "tool-x"),
        base(),
    );
    let steer = body(S, r#","turn_id":"t1","prompt":"PROMPT-ONE""#);
    drop_at(
        &mut f,
        HookEvent::UserPromptSubmit,
        &steer,
        base() + secs(1),
    );
    assert_eq!(open(&f, S), 1);
    drop_at(
        &mut f,
        HookEvent::PreToolUse,
        &call(S, "t2", "c9", "tool-z"),
        base() + secs(2),
    );
    assert_eq!(open(&f, S), 0);
}

#[test]
fn the_order_of_files_with_one_time_does_not_matter() {
    let pre = (HookEvent::PreToolUse, call(S, "t1", "c1", "tool-x"));
    let ask_ = (HookEvent::PermissionRequest, ask(S, "t1", "tool-x"));
    let post = (HookEvent::PostToolUse, call(S, "t1", "c1", "tool-x"));
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let events = [pre, ask_, post];
    for order in orders {
        let mut r = Requests::default();
        for i in order {
            r.note(events[i].0, &events[i].1, base());
        }
        r.settle();
        assert!(r.open(S).is_empty(), "order {order:?}");
    }
}

#[test]
fn a_sub_agents_request_survives_a_root_turn_change_until_stop() {
    let mut f = feed("agent");
    let sub = body(
        S,
        r#","turn_id":"t1","agent_id":"a1","tool_name":"shell","tool_input":{"command":"tool-x"}"#,
    );
    drop_at(&mut f, HookEvent::PermissionRequest, &sub, base());
    drop_at(
        &mut f,
        HookEvent::PreToolUse,
        &call(S, "t2", "c9", "tool-z"),
        base() + secs(1),
    );
    assert_eq!(open(&f, S), 1);
    assert_eq!(f.requests().open(S)[0].agent_id.as_deref(), Some("a1"));
    drop_at(
        &mut f,
        HookEvent::Stop,
        &body(S, r#","turn_id":"t2""#),
        base() + secs(2),
    );
    assert_eq!(open(&f, S), 0);
}

#[test]
fn seventeen_requests_keep_the_newest_sixteen() {
    let mut r = Requests::default();
    for n in 0..17_u64 {
        let cmd = format!("tool-{n}");
        r.note(
            HookEvent::PermissionRequest,
            &ask(S, "t1", &cmd),
            base() + secs(n),
        );
    }
    r.settle();
    let open = r.open(S);
    assert_eq!(open.len(), 16);
    assert_eq!(open[0].detail, "tool-1");
    assert_eq!(open[15].detail, "tool-16");
}

#[test]
fn input_of_the_wrong_shape_is_read_and_nothing_panics() {
    let mut f = feed("shape");
    let odd = body(S, r#","tool_input":"a string","turn_id":7"#);
    drop_at(&mut f, HookEvent::PermissionRequest, &odd, base());
    let [asked] = f.requests().open(S) else {
        panic!("one open request");
    };
    assert_eq!((asked.tool.as_str(), asked.detail.as_str()), ("", ""));
    assert_eq!(asked.turn_id, None);
    let words = body(
        S,
        r#","tool_name":"shell","tool_input":{"command":["tool-x","--flag"]}"#,
    );
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &words,
        base() + secs(1),
    );
    assert_eq!(f.requests().open(S)[1].detail, "tool-x --flag");
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        "not json",
        base() + secs(2),
    );
    assert_eq!(open(&f, S), 2);
}

#[test]
fn each_session_keeps_its_own_requests() {
    let mut f = feed("two");
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &ask("s-0001", "t1", "tool-x"),
        base(),
    );
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &ask("s-0002", "t1", "tool-x"),
        base(),
    );
    drop_at(
        &mut f,
        HookEvent::Stop,
        &body("s-0001", r#","turn_id":"t1""#),
        base() + secs(1),
    );
    assert_eq!((open(&f, "s-0001"), open(&f, "s-0002")), (0, 1));
    let seq = f.requests().open("s-0002")[0].seq;
    f.requests_mut().retire("s-0001", seq);
    assert_eq!(open(&f, "s-0002"), 1);
    f.requests_mut().retire("s-0002", seq);
    assert_eq!(open(&f, "s-0002"), 0);
}

#[test]
fn forget_drops_what_the_caller_says_is_gone() {
    let mut f = feed("forget");
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &ask(S, "t1", "tool-x"),
        base(),
    );
    f.requests_mut()
        .forget(|id, a| id == S && a.tool == "shell");
    assert_eq!(open(&f, S), 0);
}

#[test]
fn a_request_with_a_description_is_tied_to_and_retired_with_its_call() {
    let mut f = feed("description");
    drop_at(
        &mut f,
        HookEvent::PreToolUse,
        &call(S, "t1", "c1", "npm test"),
        base(),
    );
    // Codex adds the justification to a PermissionRequest's input only.
    drop_at(
        &mut f,
        HookEvent::PermissionRequest,
        &body(
            S,
            r#","turn_id":"t1","tool_name":"shell","tool_input":{"command":"npm test","description":"Run the tests"}"#,
        ),
        base() + secs(1),
    );
    assert_eq!(f.requests().open(S)[0].call_id.as_deref(), Some("c1"));
    drop_at(
        &mut f,
        HookEvent::PostToolUse,
        &call(S, "t1", "c1", "npm test"),
        base() + secs(2),
    );
    assert_eq!(open(&f, S), 0);
}
