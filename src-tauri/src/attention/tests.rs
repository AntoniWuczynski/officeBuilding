//! The reconcile's walkthroughs (spec 8.2 row 2, 8.3 row 3). Hook files are composed from F19
//! (Src 161 `hooks/src/schema.rs`) and rollout evidence from F15 to F18, not replayed:
//! placeholder ids and commands only, written with explicit times.

use super::*;
use crate::model::{EndedSession, Project, Spend};
use crate::setup::HookEvent;
use std::fs::{self, File};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub(super) const S: &str = "00000000-0000-0000-0000-000000000001";
pub(super) const T: &str = "00000000-0000-0000-0000-000000000002";
pub(super) const P: &str = "p-0001";
pub(super) const OLD: &str = "2000-01-01T00:00:00Z";

pub(super) fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn base() -> SystemTime {
    SystemTime::UNIX_EPOCH + secs(1_800_000_000)
}

pub(super) fn at(n: u64) -> SystemTime {
    base() + secs(n)
}

pub(super) fn iso(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub(super) fn session(
    id: &str,
    tool: ToolKind,
    control: ControlMode,
    state: SessionState,
) -> Session {
    Session {
        session_name: None,
        session_name_source: None,
        self_reviewed: false,
        id: id.to_string(),
        tool,
        project_id: P.to_string(),
        title: format!("desk {id}"),
        state,
        control,
        model: None,
        effort: None,
        last_reply: None,
        last_activity_at: OLD.to_string(),
        pending_question_ids: Vec::new(),
        spend: Spend::default(),
        helpers: Vec::new(),
        pets: Vec::new(),
        worktree: false,
        branch: None,
    }
}

/// A row discovery made for `session`: `suffix` "prose" for a prose ask, else a formal question.
pub(super) fn row(session: &str, suffix: &str) -> Question {
    Question {
        id: format!("{session}:{suffix}"),
        project_id: P.to_string(),
        session_id: session.to_string(),
        prompt: "QUESTION-ONE".to_string(),
        options: Vec::new(),
        allow_other: true,
        answer_via: AnswerVia::Terminal,
        priority: i32::from(suffix == "prose"),
        asked_at: OLD.to_string(),
        context: String::new(),
        source_file: None,
        handoff: None,
        asked_by: None,
    }
}

/// What discovery found: `sessions`, with each one's rows already in its pending ids.
pub(super) fn found(sessions: Vec<Session>, rows: Vec<Question>) -> Discovered {
    let mut sessions = sessions;
    for s in &mut sessions {
        s.pending_question_ids = rows
            .iter()
            .filter(|q| q.session_id == s.id)
            .map(|q| q.id.clone())
            .collect();
    }
    Discovered {
        projects: vec![Project {
            id: P.to_string(),
            name: "app".to_string(),
            path: "~/code/app".to_string(),
            session_ids: sessions.iter().map(|s| s.id.clone()).collect(),
        }],
        sessions,
        questions: rows,
        ..Discovered::default()
    }
}

/// One Codex desk in an app terminal, as discovery found it.
pub(super) fn codex(state: SessionState, rows: Vec<Question>) -> Discovered {
    found(
        vec![session(S, ToolKind::Codex, ControlMode::Full, state)],
        rows,
    )
}

pub(super) fn hooked(session: &str, turn: &str, extra: &str) -> String {
    format!(r#"{{"session_id":"{session}","turn_id":"{turn}"{extra}}}"#)
}

/// A tool call's hook input; `call` empty for a `PermissionRequest`, which names none.
pub(super) fn call(session: &str, turn: &str, call: &str, cmd: &str) -> String {
    let id = if call.is_empty() {
        String::new()
    } else {
        format!(r#","tool_use_id":"{call}""#)
    };
    hooked(
        session,
        turn,
        &format!(r#","tool_name":"shell"{id},"tool_input":{{"command":"{cmd}"}}"#),
    )
}

/// The feed and what each scan of a walkthrough produced.
pub(super) struct Walk {
    pub(super) feed: Feed,
    pub(super) out: Vec<Discovered>,
}

impl Walk {
    pub(super) fn new() -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ob-attention-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        Self {
            feed: Feed::new(dir),
            out: Vec::new(),
        }
    }

    /// Drop one hook file, dated `when`, as a tool would.
    pub(super) fn hook(&self, event: HookEvent, json: &str, when: SystemTime) {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = self.feed.dir().join(format!("{}.{n:06}", event.name()));
        fs::write(&path, json).expect("write");
        File::options()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(when))
            .expect("set the time");
    }

    /// The request a `PreToolUse` and a `PermissionRequest` of call `c1` in turn `t1` open.
    fn ask(&self, session: &str, from: u64) {
        self.hook(
            HookEvent::PreToolUse,
            &call(session, "t1", "c1", "tool-x --flag"),
            at(from),
        );
        self.hook(
            HookEvent::PermissionRequest,
            &call(session, "t1", "", "tool-x --flag"),
            at(from + 1),
        );
    }

    pub(super) fn scan(&mut self, before: &Discovered, now: SystemTime) -> Discovered {
        self.scan_with(before, now, |_| Some(Host::Nowhere))
    }

    pub(super) fn scan_with(
        &mut self,
        before: &Discovered,
        now: SystemTime,
        host: impl FnMut(&str) -> Option<Host>,
    ) -> Discovered {
        let mut found = before.clone();
        self.feed.collect(now);
        reconcile(&mut found, &mut self.feed, host, now);
        self.out.push(found.clone());
        found
    }
}

/// What A Human sees of `session`: its state, its pending ids and the ids of its rows.
pub(super) fn seen(found: &Discovered, session: &str) -> (SessionState, Vec<String>, Vec<String>) {
    let s = found
        .sessions
        .iter()
        .find(|s| s.id == session)
        .expect("the session");
    (
        s.state,
        s.pending_question_ids.clone(),
        rows_of(found, session),
    )
}

pub(super) fn rows_of(found: &Discovered, session: &str) -> Vec<String> {
    found
        .questions
        .iter()
        .filter(|q| q.session_id == session)
        .map(|q| q.id.clone())
        .collect()
}

fn waiting(ids: &[&str]) -> (SessionState, Vec<String>, Vec<String>) {
    let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
    (SessionState::WaitingHuman, ids.clone(), ids)
}

fn clear(state: SessionState) -> (SessionState, Vec<String>, Vec<String>) {
    (state, Vec::new(), Vec::new())
}

fn approval(n: u64) -> String {
    format!("{S}:approval:{n}")
}

// 8.2 rows 1 to 4a: a request opened, looked at, left alone through noise, answered.
fn request_answered_in_the_terminal() -> Vec<Discovered> {
    let mut w = Walk::new();
    let discovered = codex(SessionState::Running, Vec::new());
    w.ask(S, 0);
    let one = w.scan(&discovered, at(2));
    assert_eq!(seen(&one, S), waiting(&[&approval(1)]), "1");
    let [q] = one.questions.as_slice() else {
        panic!("one row");
    };
    assert_eq!(
        q.prompt,
        "Codex may be waiting for your approval: tool-x --flag"
    );
    assert_eq!(q.context, MAY_BE_SETTLED);
    assert_eq!(q.handoff, Some(Handoff::Docked));
    assert_eq!(q.answer_via, AnswerVia::Terminal);
    assert_eq!((q.priority, q.asked_at.as_str()), (0, iso(at(1)).as_str()));
    assert!(!q.allow_other && q.options.is_empty());

    let two = w.scan(&discovered, at(3));
    assert_eq!(
        two.questions, one.questions,
        "2 opening it leaves it pending"
    );

    w.hook(
        HookEvent::PostToolUse,
        &call(S, "t1", "c2", "tool-y"),
        at(4),
    );
    let mut newer = discovered.clone();
    newer.sessions[0].last_activity_at = iso(at(100));
    let three = w.scan(&newer, at(31 * 60 + 4));
    assert_eq!(seen(&three, S), waiting(&[&approval(1)]), "3");

    w.hook(
        HookEvent::PostToolUse,
        &call(S, "t1", "c1", "tool-x --flag"),
        at(31 * 60 + 5),
    );
    let four = w.scan(&discovered, at(31 * 60 + 6));
    assert_eq!(seen(&four, S), clear(SessionState::Thinking), "4a");
    w.out
}

// 8.2 rows 4b and 4c: the rollout proves the request over, with no hook since.
fn request_proven_over_by_the_rollout() -> Vec<Discovered> {
    let mut out = Vec::new();
    for (name, evidence) in [
        (
            "4b its turn ended",
            Evidence {
                ended_turns: vec!["t1".to_string()],
                ..Evidence::default()
            },
        ),
        (
            "4c its call is done",
            Evidence {
                turn_id: Some("t1".to_string()),
                done_calls: vec!["c1".to_string()],
                ..Evidence::default()
            },
        ),
    ] {
        let mut w = Walk::new();
        let mut discovered = codex(SessionState::Running, Vec::new());
        w.ask(S, 0);
        assert_eq!(
            seen(&w.scan(&discovered, at(2)), S).0,
            SessionState::WaitingHuman
        );
        discovered.codex.insert(S.to_string(), evidence);
        let after = w.scan(&discovered, at(3));
        assert_eq!(seen(&after, S), clear(SessionState::Running), "{name}");
        assert!(w.feed.requests().open(S).is_empty(), "{name}");
        out.extend(w.out);
    }
    out
}

// 8.2 row 5: a genuine input request from the rollout outlasts other tools and time.
fn input_request_from_the_rollout() -> Vec<Discovered> {
    let mut w = Walk::new();
    let discovered = codex(SessionState::WaitingHuman, vec![row(S, "call-0001")]);
    let id = format!("{S}:call-0001");
    w.hook(
        HookEvent::PreToolUse,
        &call(S, "t1", "call-0001", "x"),
        at(0),
    );
    assert_eq!(seen(&w.scan(&discovered, at(1)), S), waiting(&[&id]));
    w.hook(
        HookEvent::PostToolUse,
        &call(S, "t1", "c2", "tool-y"),
        at(2),
    );
    let later = w.scan(&discovered, at(31 * 60 + 2));
    assert_eq!(seen(&later, S), waiting(&[&id]));
    w.out
}

// 8.2 row 6: a clean idle finish.
fn clean_idle_finish() -> Vec<Discovered> {
    let mut w = Walk::new();
    w.hook(HookEvent::Stop, &hooked(S, "t1", ""), at(0));
    let out = w.scan(&codex(SessionState::Idle, Vec::new()), at(1));
    assert_eq!(seen(&out, S), clear(SessionState::Idle));
    w.out
}

// 8.2 row 7: a prose ask survives its own turn's `Stop` and goes when work resumes (C9).
fn prose_ask_through_stop() -> Vec<Discovered> {
    let mut w = Walk::new();
    let discovered = codex(SessionState::WaitingHuman, vec![row(S, "prose")]);
    let prose = format!("{S}:prose");
    w.hook(HookEvent::Stop, &hooked(S, "t1", ""), at(0));
    assert_eq!(seen(&w.scan(&discovered, at(1)), S), waiting(&[&prose]));
    w.hook(HookEvent::UserPromptSubmit, &hooked(S, "t2", ""), at(2));
    let resumed = w.scan(&discovered, at(3));
    assert_eq!(seen(&resumed, S), clear(SessionState::Thinking));
    w.out
}

// 8.2 row 8: a formal request outranks a prose ask.
fn prose_ask_and_request() -> Vec<Discovered> {
    let mut w = Walk::new();
    w.ask(S, 0);
    let out = w.scan(
        &codex(SessionState::WaitingHuman, vec![row(S, "prose")]),
        at(2),
    );
    assert_eq!(seen(&out, S), waiting(&[&approval(1)]));
    w.out
}

// 8.2 row 9: a terminal whose hooks never reported.
fn no_trusted_hooks() -> Vec<Discovered> {
    let mut w = Walk::new();
    let discovered = found(
        vec![session(
            S,
            ToolKind::Codex,
            ControlMode::RaiseWindow,
            SessionState::Running,
        )],
        Vec::new(),
    );
    let before = w.scan(&discovered, at(0));
    assert_eq!(seen(&before, S), clear(SessionState::Running));
    assert_eq!(before.unseen_prompts, [S]);
    w.hook(HookEvent::UserPromptSubmit, &hooked(S, "t1", ""), at(1));
    let after = w.scan(&discovered, at(2));
    assert!(after.unseen_prompts.is_empty());
    w.out
}

// 8.2 row 10: the desk of a Codex that has not written its rollout yet.
fn pre_turn_desk() -> Vec<Discovered> {
    let mut w = Walk::new();
    let id = "codex-new-4242";
    let discovered = found(
        vec![session(
            id,
            ToolKind::Codex,
            ControlMode::Full,
            SessionState::Idle,
        )],
        Vec::new(),
    );
    let out = w.scan(&discovered, at(0));
    assert_eq!(seen(&out, id), clear(SessionState::Idle));
    assert_eq!(out.unseen_prompts, [id]);
    w.out
}

// 8.2 row 11: Claude Code is applied as before, and only its handoff is added (C12).
fn claude_code_untouched() -> Vec<Discovered> {
    let mut w = Walk::new();
    let discovered = found(
        vec![session(
            T,
            ToolKind::ClaudeCode,
            ControlMode::Full,
            SessionState::Running,
        )],
        vec![row(T, "tool-0001")],
    );
    w.hook(
        HookEvent::Notification,
        &format!(r#"{{"session_id":"{T}","notification_type":"permission_prompt"}}"#),
        at(0),
    );
    let out = w.scan(&discovered, at(1));
    let mut sessions = discovered.sessions.clone();
    w.feed.apply(&mut sessions);
    assert_eq!(out.sessions[0].state, sessions[0].state);
    assert_eq!(
        out.sessions[0].pending_question_ids,
        [format!("{T}:tool-0001")]
    );
    let mut expected = discovered.questions.clone();
    expected[0].handoff = Some(Handoff::Docked);
    assert_eq!(out.questions, expected);
    assert!(out.unseen_prompts.is_empty());
    w.out
}

// 8.3 rows 1 and 7: two sessions in one folder, keyed apart; nobody is signed out or moved.
fn two_sessions_in_one_folder() -> Vec<Discovered> {
    let mut w = Walk::new();
    let mut discovered = found(
        vec![
            session(
                S,
                ToolKind::Codex,
                ControlMode::RaiseWindow,
                SessionState::Running,
            ),
            session(
                T,
                ToolKind::Codex,
                ControlMode::RaiseWindow,
                SessionState::Running,
            ),
        ],
        Vec::new(),
    );
    discovered.ended.push(EndedSession {
        id: "00000000-0000-0000-0000-000000000009".to_string(),
        tool: ToolKind::Codex,
        project_id: P.to_string(),
        title: "desk".to_string(),
        ended_at: OLD.to_string(),
        model: None,
        effort: None,
        last_reply: None,
    });
    w.ask(S, 0);
    let out = w.scan(&discovered, at(2));
    assert_eq!(seen(&out, S), waiting(&[&approval(1)]));
    assert_eq!(seen(&out, T), clear(SessionState::Running));
    assert_eq!(out.ended, discovered.ended);
    assert_eq!(out.projects, discovered.projects);
    w.out
}

// 8.3 rows 2 and 6: the handoff by launch mode, with each host asked once at most.
fn handoffs_by_launch_mode() -> Vec<Discovered> {
    let mut w = Walk::new();
    let raise = |id: &str| {
        session(
            id,
            ToolKind::ClaudeCode,
            ControlMode::RaiseWindow,
            SessionState::WaitingHuman,
        )
    };
    let read_only = session(
        "r",
        ToolKind::ClaudeCode,
        ControlMode::ReadOnly,
        SessionState::WaitingHuman,
    );
    let discovered = found(
        vec![
            raise("tab"),
            raise("app"),
            raise("none"),
            raise("bg"),
            raise("nopid"),
            raise("quiet"),
            read_only,
        ],
        vec![
            row("tab", "q1"),
            row("tab", "q2"),
            row("app", "q1"),
            row("none", "q1"),
            row("bg", "q1"),
            row("nopid", "q1"),
            row("r", "q1"),
            row("gone", "q1"),
        ],
    );
    let mut asked: Vec<String> = Vec::new();
    let out = w.scan_with(&discovered, at(0), |id| {
        asked.push(id.to_string());
        match id {
            "tab" => Some(Host::Window {
                app: "iTerm2",
                tab: true,
            }),
            "app" => Some(Host::Window {
                app: "Ghostty",
                tab: false,
            }),
            "none" => Some(Host::Nowhere),
            "bg" => Some(Host::Background),
            _ => None,
        }
    });
    let handoff = |id: &str| {
        out.questions
            .iter()
            .find(|q| q.id == id)
            .and_then(|q| q.handoff.clone())
    };
    let window = |app: &str, tab: bool| {
        Some(Handoff::Window {
            app: app.to_string(),
            tab,
        })
    };
    assert_eq!(handoff("tab:q1"), window("iTerm2", true));
    assert_eq!(handoff("tab:q2"), window("iTerm2", true));
    assert_eq!(handoff("app:q1"), window("Ghostty", false));
    assert_eq!(handoff("none:q1"), Some(Handoff::Elsewhere));
    assert_eq!(handoff("bg:q1"), Some(Handoff::Docked));
    assert_eq!(handoff("nopid:q1"), Some(Handoff::Elsewhere));
    assert_eq!(handoff("r:q1"), Some(Handoff::ReadOnly));
    assert_eq!(handoff("gone:q1"), Some(Handoff::Elsewhere));
    asked.sort();
    assert_eq!(asked, ["app", "bg", "none", "nopid", "tab"]);
    w.out
}

// 8.3 row 3: a session missing from one scan keeps its request, and its row comes back.
fn missing_from_one_scan() -> Vec<Discovered> {
    let mut w = Walk::new();
    let discovered = codex(SessionState::Running, Vec::new());
    w.ask(S, 0);
    let first = w.scan(&discovered, at(2));
    assert_eq!(seen(&first, S), waiting(&[&approval(1)]));
    let gone = w.scan(&found(Vec::new(), Vec::new()), at(3));
    assert!(gone.questions.is_empty());
    let back = w.scan(&discovered, at(4));
    assert_eq!(back.questions, first.questions);
    w.out
}

// 8.3 row 4: evidence for a session id that names no desk.
fn evidence_for_no_desk() -> Vec<Discovered> {
    let mut w = Walk::new();
    let stranger = "00000000-0000-0000-0000-000000000099";
    let discovered = codex(SessionState::Running, Vec::new());
    w.ask(stranger, 0);
    let out = w.scan(&discovered, at(2));
    assert!(out.questions.is_empty());
    assert_eq!(seen(&out, S), clear(SessionState::Running));
    assert_eq!(w.feed.requests().open(stranger).len(), 1, "kept");
    w.scan(&discovered, at(12 * 3600));
    assert_eq!(
        w.feed.requests().open(stranger).len(),
        1,
        "12 hours is not over"
    );
    w.scan(&discovered, at(12 * 3600 + 2));
    assert!(w.feed.requests().open(stranger).is_empty(), "dropped");
    w.out
}

// 8.3 row 5: no hooks, and an open `request_user_input` call in the rollout.
fn outside_session_with_a_question() -> Vec<Discovered> {
    let mut w = Walk::new();
    let discovered = found(
        vec![session(
            S,
            ToolKind::Codex,
            ControlMode::RaiseWindow,
            SessionState::WaitingHuman,
        )],
        vec![row(S, "call-0001")],
    );
    let out = w.scan_with(&discovered, at(0), |_| {
        Some(Host::Window {
            app: "Terminal",
            tab: true,
        })
    });
    let id = format!("{S}:call-0001");
    assert_eq!(seen(&out, S), waiting(&[&id]));
    assert_eq!(
        out.questions[0].handoff,
        Some(Handoff::Window {
            app: "Terminal".to_string(),
            tab: true
        })
    );
    assert_eq!(out.unseen_prompts, [S]);
    w.out
}

pub(super) type Case = (&'static str, fn() -> Vec<Discovered>);

const CASES: [Case; 14] = [
    ("8.2 1-4a", request_answered_in_the_terminal),
    ("8.2 4b 4c", request_proven_over_by_the_rollout),
    ("8.2 5", input_request_from_the_rollout),
    ("8.2 6", clean_idle_finish),
    ("8.2 7", prose_ask_through_stop),
    ("8.2 8", prose_ask_and_request),
    ("8.2 9", no_trusted_hooks),
    ("8.2 10", pre_turn_desk),
    ("8.2 11", claude_code_untouched),
    ("8.3 1 7", two_sessions_in_one_folder),
    ("8.3 2 6", handoffs_by_launch_mode),
    ("8.3 3", missing_from_one_scan),
    ("8.3 4", evidence_for_no_desk),
    ("8.3 5", outside_session_with_a_question),
];

#[test]
fn every_walkthrough_holds() {
    for (name, case) in CASES {
        assert!(!case().is_empty(), "{name}");
    }
}

// 8.2 row 12: for every Codex session, the hand is up exactly when it has pending ids, and
// each pending id is exactly one row of that session and floor.
pub(super) fn assert_hand_matches_rows(name: &str, found: &Discovered) {
    for s in found.sessions.iter().filter(|s| s.tool == ToolKind::Codex) {
        assert_eq!(
            s.state == SessionState::WaitingHuman,
            !s.pending_question_ids.is_empty(),
            "{name}: {}",
            s.id
        );
        for id in &s.pending_question_ids {
            let rows = found
                .questions
                .iter()
                .filter(|q| &q.id == id && q.session_id == s.id && q.project_id == s.project_id)
                .count();
            assert_eq!(rows, 1, "{name}: {id}");
        }
        assert_eq!(
            rows_of(found, &s.id).len(),
            s.pending_question_ids.len(),
            "{name}: no row outlives the hand"
        );
    }
}

#[test]
fn a_codex_hand_is_up_exactly_while_it_has_rows() {
    for (name, case) in CASES {
        for found in case() {
            assert_hand_matches_rows(name, &found);
        }
    }
}
