//! The delivery table of stage M1 (rules A1 to A6): when a desk in an app terminal takes a
//! note, for a Claude Code desk and, where the rule is the same, a Codex desk.

use super::super::tests::floor_office;
use super::tests::{hold_of, queue, screen, seat, state_of, wait_for};
use super::*;
use std::fs::{self, File};
use std::sync::atomic::{AtomicUsize, Ordering};

const NOTE: &str = "Take the next TODO item.";
/// The quiet the silence rule waits for in these tests, and how long a silent row waits.
const QUIET: Duration = Duration::from_millis(200);
const SILENCE: Duration = Duration::from_millis(300);

/// A hook event a row drops: its name, the extra JSON fields, and how old its file is.
struct Ev(&'static str, &'static str, u64);

/// The silence rule's part in a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quiet {
    /// The app's own quiet: the rule never applies within a test.
    Untouched,
    /// The quiet is shortened to `QUIET` and the row waits `SILENCE` before it looks.
    Silent,
    /// As `Silent`, then the terminal prints just before the look.
    Printed,
    /// As `Silent`, then the session's last activity is stamped now.
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Typed,
    Held(Hold),
    /// The session is put on another floor after the note is queued: its log entry says
    /// dropped and nothing is typed.
    Dropped,
}

struct Row {
    name: &'static str,
    codex_too: bool,
    state: SessionState,
    /// The suffix of the one question the session has open.
    question: Option<&'static str>,
    events: &'static [Ev],
    quiet: Quiet,
    /// What A Human sends to the terminal before the look; the keystroke stamp is then cleared.
    keys: &'static [&'static str],
    expect: Expect,
}

const STOP: &[Ev] = &[Ev("Stop", "", 0)];
const PRE_TOOL: &[Ev] = &[Ev("PreToolUse", "", 0)];

const BASE: Row = Row {
    name: "",
    codex_too: true,
    state: SessionState::Idle,
    question: None,
    events: STOP,
    quiet: Quiet::Untouched,
    keys: &[],
    expect: Expect::Typed,
};

const ROWS: [Row; 16] = [
    Row {
        name: "1 a prose ask while the registry says waiting",
        state: SessionState::WaitingHuman,
        question: Some("prose"),
        ..BASE
    },
    Row {
        name: "2 a prose ask while idle",
        question: Some("prose"),
        ..BASE
    },
    Row {
        name: "3 a formal question",
        state: SessionState::WaitingHuman,
        question: Some("toolu_1"),
        expect: Expect::Held(Hold::WaitingOnYou),
        ..BASE
    },
    Row {
        name: "4 a reported wait",
        state: SessionState::WaitingHuman,
        question: Some("waiting"),
        expect: Expect::Held(Hold::WaitingOnYou),
        ..BASE
    },
    Row {
        name: "5 a permission prompt with a prose ask",
        codex_too: false,
        state: SessionState::WaitingHuman,
        question: Some("prose"),
        events: &[Ev(
            "Notification",
            r#","notification_type":"permission_prompt""#,
            0,
        )],
        expect: Expect::Held(Hold::WaitingOnYou),
        ..BASE
    },
    Row {
        name: "6 a start mark dropped now",
        codex_too: false,
        events: &[Ev("SessionStart", r#","source":"startup""#, 0)],
        expect: Expect::Held(Hold::Busy),
        ..BASE
    },
    Row {
        name: "7 a start mark eleven seconds old",
        codex_too: false,
        events: &[Ev("SessionStart", r#","source":"startup""#, 11)],
        ..BASE
    },
    Row {
        name: "8 a start after compaction",
        codex_too: false,
        events: &[Ev("SessionStart", r#","source":"compact""#, 0)],
        expect: Expect::Held(Hold::NoIdleSignal),
        ..BASE
    },
    Row {
        name: "9 interrupted and silent",
        events: PRE_TOOL,
        quiet: Quiet::Silent,
        ..BASE
    },
    Row {
        name: "10 interrupted but the terminal just printed",
        events: PRE_TOOL,
        quiet: Quiet::Printed,
        expect: Expect::Held(Hold::Busy),
        ..BASE
    },
    Row {
        name: "11 interrupted but active just now",
        events: PRE_TOOL,
        quiet: Quiet::Active,
        expect: Expect::Held(Hold::Busy),
        ..BASE
    },
    Row {
        name: "12 its own files say running",
        state: SessionState::Running,
        events: PRE_TOOL,
        quiet: Quiet::Silent,
        expect: Expect::Held(Hold::Busy),
        ..BASE
    },
    Row {
        name: "13 an API error",
        codex_too: false,
        events: &[Ev("StopFailure", "", 0)],
        quiet: Quiet::Silent,
        expect: Expect::Held(Hold::Busy),
        ..BASE
    },
    Row {
        name: "14 Escape alone",
        keys: &["\x1b"],
        ..BASE
    },
    Row {
        name: "15 a line, then Escape",
        keys: &["abc", "\x1b"],
        expect: Expect::Held(Hold::YouTyped),
        ..BASE
    },
    Row {
        name: "16 moved to another floor",
        expect: Expect::Dropped,
        ..BASE
    },
];

/// Drop a hook event for `session_id` into the feed's folder, dated `age` seconds ago, and collect it.
fn drop_event(office: &Office, session_id: &str, ev: &Ev) {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let Ev(name, extra, age) = ev;
    let dir = office.lock().feed.dir().to_path_buf();
    fs::create_dir_all(&dir).unwrap();
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{name}.{n:06}"));
    fs::write(&path, format!(r#"{{"session_id":"{session_id}"{extra}}}"#)).unwrap();
    let at = SystemTime::now() - Duration::from_secs(*age);
    File::options()
        .write(true)
        .open(&path)
        .and_then(|f| f.set_modified(at))
        .unwrap();
    office.lock().feed.collect(SystemTime::now());
}

fn run(row: &Row, tool: ToolKind) {
    let what = format!("{} ({tool:?})", row.name);
    let tag = format!("idle-{}-{tool:?}", row.name.split(' ').next().unwrap_or(""));
    let (office, dir, floor) = floor_office(&tag);
    let (terminal, id) = seat(&office, &dir, &floor, tool, row.state);
    {
        let mut inner = office.lock();
        let s = inner
            .found
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap();
        if let Some(suffix) = row.question {
            s.pending_question_ids.push(format!("{id}:{suffix}"));
        }
        if row.quiet != Quiet::Untouched {
            inner.mail.quiet = Some(QUIET);
        }
    }
    for ev in row.events {
        drop_event(&office, &id, ev);
    }
    for keys in row.keys {
        office.terminal_write(&id, keys).unwrap();
    }
    office.lock().mail.typed.remove(&terminal);
    if row.quiet != Quiet::Untouched {
        std::thread::sleep(SILENCE);
    }
    if row.quiet == Quiet::Active {
        let mut inner = office.lock();
        let s = inner
            .found
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap();
        // To the millisecond: `now_iso` drops them, which can read as a second old.
        s.last_activity_at =
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    }
    if row.quiet == Quiet::Printed {
        office.terminals.write(&terminal, "printed").unwrap();
        wait_for("the echo", || {
            screen(&office, &terminal).contains("printed")
        });
    }
    let note = queue(&office, &floor, &id, NOTE);
    match row.expect {
        Expect::Typed => {
            assert_eq!(hold_of(&office, &floor, &id), None, "{what}");
            assert!(office.deliver_due(), "{what}");
            assert_eq!(
                state_of(&office, &note),
                Some(NoteState::Delivered),
                "{what}"
            );
            wait_for(&what, || screen(&office, &terminal).contains(NOTE));
        }
        Expect::Held(hold) => {
            assert_eq!(hold_of(&office, &floor, &id), Some(hold), "{what}");
            office.deliver_due();
            assert_eq!(state_of(&office, &note), Some(NoteState::Queued), "{what}");
        }
        Expect::Dropped => {
            let mut inner = office.lock();
            let s = inner
                .found
                .sessions
                .iter_mut()
                .find(|s| s.id == id)
                .unwrap();
            s.project_id = "p-elsewhere".to_string();
            drop(inner);
            assert!(office.deliver_due(), "{what}: a dropped note shows");
            assert_eq!(state_of(&office, &note), Some(NoteState::Dropped), "{what}");
        }
    }
    if row.expect != Expect::Typed {
        assert!(!screen(&office, &terminal).contains(NOTE), "{what}");
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_delivery_table() {
    for row in &ROWS {
        run(row, ToolKind::ClaudeCode);
        if row.codex_too {
            run(row, ToolKind::Codex);
        }
    }
}

/// A structured desk whose every turn fails to start.
struct Refuses;

impl crate::structured::Driver for Refuses {
    fn prompt(&self, _: &str) -> Result<(), String> {
        Err("no".to_string())
    }
    fn answer(&self, _: &str, _: &str, _: &crate::structured::Values) -> Result<(), String> {
        Ok(())
    }
    fn cancel(&self) -> Result<(), String> {
        Ok(())
    }
    fn stop(&self) {}
}

/// `send_to_desk`'s answer. Here, not in `manager_send.rs`, because the guard keeps the human's
/// own composer out of the manager's files, tests included.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Answer {
    outcome: String,
    note_id: String,
    desk: String,
    waiting_for: Option<Hold>,
    detail: String,
}

#[test]
fn a_managers_note_behind_the_humans_own_message_is_on_its_way() {
    use super::super::manager::tests::{manager_office, seat as seat_manager};
    use crate::manager::tools::{Call, SendToDesk, Tools};
    use crate::structured::desks::DeskInfo;
    let (office, dir, floor, registry, _) = manager_office("idle-behind");
    seat_manager(&office, &dir, &floor, &registry, "/bin/cat");
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
        .seat("structured-1", info, SessionState::Idle, Box::new(Refuses));
    office.chat_send("structured-1", "mine first").unwrap();
    assert_eq!(office.lock().mail.own.items().len(), 1, "it keeps failing");
    let call = Call::SendToDesk(SendToDesk {
        desk: "structured-1".to_string(),
        text: "go".to_string(),
    });
    let answer: Answer = serde_json::from_str(&office.call(&floor.id, call).unwrap()).unwrap();
    assert_eq!(
        (
            answer.outcome.as_str(),
            answer.waiting_for,
            answer.desk.as_str()
        ),
        ("on-its-way", None, "structured-1")
    );
    assert!(answer.note_id.starts_with("n-"));
    assert!(
        answer.detail.contains("handing it over"),
        "{}",
        answer.detail
    );
    fs::remove_dir_all(&dir).unwrap();
}
