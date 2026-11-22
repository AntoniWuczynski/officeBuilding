//! The join (M2b spec 10.5): discovery's finds and the chat desks joined by thread id before
//! anything is deduplicated. Pure cases on a hand-built `Discovered` and a seated desk, then
//! cases through an `Office` on synthetic rollouts (G1, G2 shapes). Ids are placeholders.

use super::super::super::deliver::tests::session;
use super::super::tests::{DESK, Fake, seated};
use super::*;
use crate::handover::Tool;
use crate::handover::view::TakeOverStart;
use crate::manager::tools::{Call, Tools};
use crate::model::{ControlMode, Pet, PetKind, Place, ToolKind};
use crate::office::Office;
use crate::structured::desks::DeskInfo;
use crate::structured::{Approval, Event};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const FLOOR: &str = "p-1";
const A: u32 = 2;
const B: u32 = 3;
const C: u32 = 4;

fn uuid(n: u32) -> String {
    format!("0199c0de-0000-4000-8000-{n:012}")
}

/// The chat desk's own Codex thread.
fn thread() -> String {
    uuid(1)
}

fn floor() -> Project {
    Project {
        id: FLOOR.to_string(),
        name: "app".to_string(),
        path: "/tmp/app".to_string(),
        session_ids: Vec::new(),
    }
}

fn helper(n: u32, state: SessionState) -> Helper {
    Helper {
        id: uuid(n),
        state,
        worktree: false,
        branch: None,
        model: None,
        effort: None,
    }
}

fn ids<T>(items: &[T], id: impl Fn(&T) -> &str) -> Vec<String> {
    items.iter().map(|i| id(i).to_string()).collect()
}

/// What 7.4 says must hold of every `Joined`. `running`: the running desks' threads.
fn invariant(case: &str, j: &Joined, running: &[String]) {
    let sessions = ids(&j.sessions, |s| s.id.as_str());
    let ended = ids(&j.ended, |e| e.id.as_str());
    let unique: BTreeSet<&String> = sessions.iter().collect();
    assert_eq!(unique.len(), sessions.len(), "{case}: a session twice");
    for s in &j.sessions {
        assert!(
            !ended.contains(&s.id),
            "{case}: {} live and signed out",
            s.id
        );
        let helpers: BTreeSet<&str> = s.helpers.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(helpers.len(), s.helpers.len(), "{case}: a helper twice");
        for h in &helpers {
            let h = (*h).to_string();
            assert!(!sessions.contains(&h), "{case}: helper {h} is a session");
            assert!(!ended.contains(&h), "{case}: helper {h} is signed out");
        }
    }
    for q in &j.questions {
        assert!(
            !running.contains(&q.session_id),
            "{case}: row {} names a running desk's thread",
            q.id
        );
    }
}

/// How the case's chat desk is seated.
#[derive(Clone, Copy)]
enum Seat {
    /// Seated, idle, with its thread reported.
    Thread,
    /// Seated, idle, with no thread reported yet.
    NoThread,
    /// No chat desk at all.
    None,
}

struct Case {
    name: &'static str,
    seat: Seat,
    setup: fn(&mut Discovered, &Desks),
    expect: fn(&str, &Joined),
}

/// Adds `N`, the discovered session of the desk's thread, to the floor.
fn add_n(found: &mut Discovered, helpers: Vec<Helper>) {
    add_session(found, &thread(), ToolKind::Codex, helpers);
}

fn add_session(found: &mut Discovered, id: &str, tool: ToolKind, helpers: Vec<Helper>) {
    let mut s = session(id, &floor(), tool, ControlMode::RaiseWindow);
    s.helpers = helpers;
    found.projects[0].session_ids.push(s.id.clone());
    found.sessions.push(s);
}

fn row(session_id: &str, suffix: &str) -> Question {
    Question {
        id: format!("{session_id}:{suffix}"),
        project_id: "p-other".to_string(),
        session_id: session_id.to_string(),
        prompt: "QUESTION-ONE".to_string(),
        options: Vec::new(),
        allow_other: true,
        answer_via: AnswerVia::Terminal,
        priority: 1,
        asked_at: "2026-11-20T10:00:00Z".to_string(),
        context: String::new(),
        source_file: None,
        handoff: None,
        asked_by: None,
    }
}

fn line(id: &str, at: &str) -> EndedSession {
    EndedSession {
        id: id.to_string(),
        tool: ToolKind::Codex,
        project_id: FLOOR.to_string(),
        title: "t".to_string(),
        ended_at: at.to_string(),
        model: None,
        effort: None,
        last_reply: None,
    }
}

fn desk(j: &Joined) -> &Session {
    j.sessions.iter().find(|s| s.id == DESK).expect("the desk")
}

fn helper_ids(case: &str, j: &Joined) -> Vec<String> {
    let d = j.sessions.iter().find(|s| s.id == DESK);
    ids(&d.expect(case).helpers, |h| h.id.as_str())
}

fn driver_has(desks: &Desks, helpers: Vec<Helper>) {
    desks.on_event(DESK, Event::Helpers(helpers));
}

fn state_of(case: &str, j: &Joined, n: u32) -> SessionState {
    let h = desk(j).helpers.iter().find(|h| h.id == uuid(n));
    h.unwrap_or_else(|| panic!("{case}: helper {n}")).state
}

fn helper_cases() -> Vec<Case> {
    use SessionState::{Running, WaitingHuman};
    vec![
        Case {
            name: "1 the disk's helpers land on the desk",
            seat: Seat::Thread,
            setup: |f, _| add_n(f, vec![helper(A, Running), helper(B, Running)]),
            expect: |case, j| {
                assert_eq!(helper_ids(case, j), [uuid(A), uuid(B)], "{case}");
                assert!(!j.sessions.iter().any(|s| s.id == thread()), "{case}");
                let listed = &j.projects[0].session_ids;
                assert_eq!(
                    listed,
                    &[DESK.to_string()],
                    "{case}: the floor lists the desk once"
                );
            },
        },
        Case {
            name: "2 two observations of one worker",
            seat: Seat::Thread,
            setup: |f, d| {
                add_n(f, vec![helper(A, Running)]);
                driver_has(d, vec![helper(A, Running)]);
            },
            expect: |case, j| assert_eq!(helper_ids(case, j), [uuid(A)], "{case}"),
        },
        Case {
            name: "3 a worker only the driver has seen",
            seat: Seat::Thread,
            setup: |f, d| {
                add_n(f, vec![helper(A, Running), helper(B, Running)]);
                driver_has(d, vec![helper(C, Running)]);
            },
            expect: |case, j| {
                assert_eq!(helper_ids(case, j), [uuid(A), uuid(B), uuid(C)], "{case}");
            },
        },
        Case {
            name: "4 the disk read a worker and no longer draws it",
            seat: Seat::Thread,
            setup: |f, d| {
                add_n(f, Vec::new());
                f.codex_gone.insert(thread(), BTreeSet::from([uuid(A)]));
                driver_has(d, vec![helper(A, Running)]);
            },
            expect: |case, j| assert!(helper_ids(case, j).is_empty(), "{case}"),
        },
        Case {
            name: "4b the disk stopped drawing a worker it does not prove over",
            seat: Seat::Thread,
            setup: |f, d| {
                add_n(f, Vec::new());
                driver_has(d, vec![helper(A, Running)]);
            },
            expect: |case, j| assert_eq!(helper_ids(case, j), [uuid(A)], "{case}"),
        },
        Case {
            name: "5 the same, and the driver's worker is asking",
            seat: Seat::Thread,
            setup: |f, d| {
                add_n(f, Vec::new());
                f.codex_gone.insert(thread(), BTreeSet::from([uuid(A)]));
                driver_has(d, vec![helper(A, WaitingHuman)]);
            },
            expect: |case, j| assert_eq!(state_of(case, j, A), WaitingHuman, "{case}"),
        },
        Case {
            name: "6 the disk has it running and the driver has it asking",
            seat: Seat::Thread,
            setup: |f, d| {
                add_n(f, vec![helper(A, Running)]);
                driver_has(d, vec![helper(A, WaitingHuman)]);
            },
            expect: |case, j| {
                assert_eq!(helper_ids(case, j), [uuid(A)], "{case}");
                assert_eq!(state_of(case, j, A), WaitingHuman, "{case}");
            },
        },
        Case {
            name: "7 the thread's pets",
            seat: Seat::Thread,
            setup: |f, _| {
                add_n(f, Vec::new());
                f.sessions[0].pets = ["pet-1", "pet-2"]
                    .map(|id| Pet {
                        id: id.to_string(),
                        kind: PetKind::Shell,
                        label: "sleep".to_string(),
                    })
                    .to_vec();
            },
            expect: |case, j| {
                assert_eq!(
                    ids(&desk(j).pets, |p| p.id.as_str()),
                    ["pet-1", "pet-2"],
                    "{case}"
                );
            },
        },
    ]
}

fn prose_rows(f: &mut Discovered) {
    add_n(f, Vec::new());
    f.questions.push(row("s-other", "call-1"));
    f.questions.push(row(&thread(), "prose"));
}

fn row_cases() -> Vec<Case> {
    vec![
        Case {
            name: "8 an idle desk's prose ask moves to the desk",
            seat: Seat::Thread,
            setup: |f, _| prose_rows(f),
            expect: |case, j| {
                assert_eq!(
                    ids(&j.questions, |q| q.id.as_str()),
                    ["s-other:call-1", "structured-1:prose"],
                    "{case}"
                );
                let q = &j.questions[1];
                assert_eq!(
                    (q.session_id.as_str(), q.project_id.as_str()),
                    (DESK, FLOOR),
                    "{case}"
                );
                assert_eq!(q.answer_via, AnswerVia::Chat, "{case}");
                assert_eq!(q.handoff, Some(Handoff::Conversation), "{case}");
                assert!(!q.allow_other, "{case}");
                assert_eq!(
                    desk(j).pending_question_ids,
                    std::slice::from_ref(&q.id),
                    "{case}"
                );
            },
        },
        Case {
            name: "9 a thinking desk's prose ask is not a row",
            seat: Seat::Thread,
            setup: |f, d| {
                prose_rows(f);
                d.on_event(DESK, Event::State(SessionState::Thinking));
            },
            expect: |case, j| {
                assert_eq!(
                    ids(&j.questions, |q| q.id.as_str()),
                    ["s-other:call-1"],
                    "{case}"
                );
                assert!(desk(j).pending_question_ids.is_empty(), "{case}");
            },
        },
        Case {
            name: "10 a formal row of the thread is the desk's request seen twice",
            seat: Seat::Thread,
            setup: |f, d| {
                add_n(f, Vec::new());
                f.questions.push(row(&thread(), "call-1"));
                let ask = Approval {
                    id: "1".to_string(),
                    title: "ASK-1".to_string(),
                    ..Approval::default()
                };
                d.on_event(DESK, Event::Approval(ask));
                d.on_event(DESK, Event::State(SessionState::WaitingHuman));
            },
            expect: |case, j| {
                assert_eq!(
                    ids(&j.questions, |q| q.id.as_str()),
                    ["structured-1:request:1"],
                    "{case}"
                );
            },
        },
    ]
}

fn ended_ids(j: &Joined) -> Vec<String> {
    ids(&j.ended, |e| e.id.as_str())
}

fn session_ids(j: &Joined) -> Vec<String> {
    ids(&j.sessions, |s| s.id.as_str())
}

fn exit(d: &Desks) {
    d.on_event(
        DESK,
        Event::Exited {
            code: Some(0),
            message: None,
        },
    );
}

fn sheet_cases() -> Vec<Case> {
    vec![
        Case {
            name: "11 a running desk's thread is not signed out",
            seat: Seat::Thread,
            setup: |f, _| f.ended.push(line(&thread(), "2026-11-20T09:00:00Z")),
            expect: |case, j| {
                assert!(ended_ids(j).is_empty(), "{case}");
                assert_eq!(session_ids(j), [DESK], "{case}");
            },
        },
        Case {
            name: "12 an ended desk whose thread runs on in a terminal",
            seat: Seat::Thread,
            setup: |f, d| {
                exit(d);
                add_n(f, Vec::new());
            },
            expect: |case, j| {
                assert_eq!(session_ids(j), [thread()], "{case}");
                assert!(ended_ids(j).is_empty(), "{case}");
            },
        },
        Case {
            name: "13 an ended desk is on the sheet once, as the desk",
            seat: Seat::Thread,
            setup: |f, d| {
                exit(d);
                f.ended.push(line(&thread(), "2026-11-20T09:00:00Z"));
            },
            expect: |case, j| assert_eq!(ended_ids(j), [DESK], "{case}"),
        },
        Case {
            name: "14 the driver sees a chat desk's prompts",
            seat: Seat::Thread,
            setup: |f, _| f.unseen_prompts = vec![thread(), "s-other".to_string()],
            expect: |case, j| assert_eq!(j.unseen_prompts, ["s-other"], "{case}"),
        },
        Case {
            name: "15 a desk whose thread is not known yet",
            seat: Seat::NoThread,
            setup: |f, _| add_n(f, vec![helper(A, SessionState::Running)]),
            expect: |case, j| {
                assert_eq!(session_ids(j), [thread(), DESK.to_string()], "{case}");
                assert_eq!(
                    j.sessions[0].helpers.len(),
                    1,
                    "{case}: discovery's untouched"
                );
                assert!(desk(j).helpers.is_empty(), "{case}");
            },
        },
        Case {
            name: "16 a Claude Code session and no chat desk",
            seat: Seat::None,
            setup: |f, _| {
                add_session(
                    f,
                    "s-claude",
                    ToolKind::ClaudeCode,
                    vec![helper(A, SessionState::Running)],
                );
            },
            expect: |case, j| {
                assert_eq!(session_ids(j), ["s-claude"], "{case}");
                assert_eq!(
                    j.sessions[0].helpers,
                    [helper(A, SessionState::Running)],
                    "{case}"
                );
                assert_eq!(j.projects[0].session_ids, ["s-claude"], "{case}");
            },
        },
    ]
}

fn seated_desks(seat: Seat) -> Arc<Desks> {
    let desks = Desks::new(Arc::new(|_| {}));
    if matches!(seat, Seat::None) {
        return desks;
    }
    let info = DeskInfo {
        tool: ToolKind::Codex,
        project_id: FLOOR.to_string(),
        root: PathBuf::from("/tmp/app"),
        title: "TASK-ONE".to_string(),
        model: "default".to_string(),
        effort: "default".to_string(),
    };
    desks.seat(
        DESK,
        info,
        SessionState::Idle,
        Box::new(Fake(Arc::default())),
    );
    if matches!(seat, Seat::Thread) {
        desks.on_event(DESK, Event::SessionId(thread()));
    }
    desks
}

#[test]
fn the_join_projects_a_thread_onto_its_desk_before_deduplicating() {
    let cases = [helper_cases(), row_cases(), sheet_cases()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 17);
    for case in cases {
        let desks = seated_desks(case.seat);
        let mut found = Discovered {
            projects: vec![floor()],
            ..Discovered::default()
        };
        (case.setup)(&mut found, &desks);
        let joined = join(&found, &desks, Vec::new());
        (case.expect)(case.name, &joined);
        invariant(case.name, &joined, &desks.tool_sessions());
    }
}

/// An office whose seated chat desk owns thread 1, written 13 hours ago, and whose worker
/// (thread `A`, G2's nested link) has a turn open in a linked worktree.
struct Building {
    office: Office,
    dir: PathBuf,
    floor: Project,
    tree: PathBuf,
}

fn write_rollout(dir: &Path, n: u32, lines: &[String], age: Duration) {
    let path = dir.join(format!(
        "home/.codex/sessions/2026/09/29/rollout-2026-09-29T10-00-00-{}.jsonl",
        uuid(n)
    ));
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write");
    fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|f| f.set_modified(SystemTime::now() - age))
        .expect("mtime");
}

fn building(tag: &str, state: SessionState) -> Building {
    let (office, dir, floor, _) = seated(tag, state);
    let repo = dir.join("home/code/app");
    let tree = dir.join("home/code/tree-b");
    let git_dir = repo.join(".git/worktrees/b");
    fs::create_dir_all(&git_dir).expect("git dir");
    fs::create_dir_all(&tree).expect("tree");
    fs::create_dir_all(dir.join("home/.codex/sessions/2026/09/29")).expect("sessions");
    fs::write(git_dir.join("commondir"), "../..\n").expect("write");
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/b-branch\n").expect("write");
    fs::write(
        tree.join(".git"),
        format!("gitdir: {}\n", git_dir.display()),
    )
    .expect("write");
    // G1: a session of its own.
    let (root, cwd) = (thread(), repo.display());
    let meta = format!(
        r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{root}","session_id":"{root}","cwd":"{cwd}","source":"cli"}}}}"#
    );
    write_rollout(&dir, 1, &[meta], Duration::from_hours(13));
    // G2: a worker in 0.161.0's nested encoding, with a turn open.
    let (id, cwd) = (uuid(A), tree.display());
    let worker = format!(
        r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","cwd":"{cwd}","source":{{"subagent":{{"thread_spawn":{{"parent_thread_id":"{root}","depth":1,"agent_path":"/root/scout"}}}}}}}}}}"#
    );
    let started =
        r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
    write_rollout(
        &dir,
        A,
        &[worker, started.to_string()],
        Duration::from_secs(1),
    );
    office.structured.on_event(DESK, Event::SessionId(thread()));
    office.refresh();
    Building {
        office,
        dir,
        floor,
        tree,
    }
}

struct OfficeCase {
    name: &'static str,
    state: SessionState,
    check: fn(&str, &Building),
}

/// The desk and its worker, as the snapshot and the alert tracker see them.
fn office_cases() -> Vec<OfficeCase> {
    vec![
        OfficeCase {
            name: "17 an idle desk keeps its thread off the sheet and draws its worker",
            state: SessionState::Idle,
            check: |case, b| {
                let snapshot = b.office.snapshot();
                assert!(!ended_ids_of(&snapshot.ended).contains(&thread()), "{case}");
                let d = snapshot.sessions.iter().find(|s| s.id == DESK).expect(case);
                assert_eq!(ids(&d.helpers, |h| h.id.as_str()), [uuid(A)], "{case}");
                let place = Place::Helper {
                    session_id: DESK.to_string(),
                    helper_id: uuid(A),
                };
                assert_eq!(b.office.folder_of(&place), Ok(b.tree.clone()), "{case}");
                let states = b.office.desk_states();
                assert_eq!(ids(&states, |s| s.id.as_str()), [DESK], "{case}");
            },
        },
        OfficeCase {
            name: "18 a worker's request on the desk",
            state: SessionState::WaitingHuman,
            check: |case, b| {
                let ask = Approval {
                    id: "1".to_string(),
                    title: "ASK-1".to_string(),
                    asked_by: Some("scout".to_string()),
                    ..Approval::default()
                };
                b.office.structured.on_event(DESK, Event::Approval(ask));
                driver_has(
                    &b.office.structured,
                    vec![helper(A, SessionState::WaitingHuman)],
                );
                let rows: Vec<Question> = b.office.snapshot().questions;
                assert_eq!(rows.len(), 1, "{case}");
                assert_eq!(rows[0].asked_by.as_deref(), Some("scout"), "{case}");
                let states = b.office.desk_states();
                assert_eq!(
                    ids(&states, |s| s.id.as_str()),
                    [DESK],
                    "{case}: no entry for the worker"
                );
                assert_eq!(states[0].state, SessionState::WaitingHuman, "{case}");
            },
        },
    ]
}

/// The cases of 7.7: what cannot act on a helper.
fn helper_cases_in_an_office() -> Vec<OfficeCase> {
    vec![
        OfficeCase {
            name: "19 nothing in the building acts on a helper",
            state: SessionState::Idle,
            check: |case, b| {
                let (o, w) = (&b.office, uuid(A));
                let start = TakeOverStart {
                    tool: Tool::ClaudeCode,
                    model: "m".to_string(),
                    effort: "high".to_string(),
                };
                let tried = [
                    ("resume", o.resume_session(&w)),
                    ("hand over", o.hand_over_desk(&w)),
                    ("exile", o.exile_session(&w)),
                    ("move", o.move_session(&w, Some(&b.floor.id))),
                    ("take over", o.take_over_desk(&w, &start)),
                    (
                        "folder",
                        o.folder_of(&Place::Session {
                            session_id: w.clone(),
                        })
                        .map(|_| ()),
                    ),
                ];
                for (action, result) in tried {
                    assert!(result.is_err(), "{case}: {action} accepted a helper");
                }
                assert!(o.desk_now(&o.lock(), &w).is_none(), "{case}");
                let snapshot = o.snapshot();
                assert!(!snapshot.sessions.iter().any(|s| s.id == w), "{case}");
                assert!(!ended_ids_of(&snapshot.ended).contains(&w), "{case}");
            },
        },
        OfficeCase {
            name: "20 the manager's list_desks names no helper",
            state: SessionState::Running,
            check: |case, b| {
                driver_has(&b.office.structured, vec![helper(B, SessionState::Running)]);
                let d = b
                    .office
                    .snapshot()
                    .sessions
                    .into_iter()
                    .find(|s| s.id == DESK);
                assert_eq!(d.expect(case).helpers.len(), 2, "{case}");
                let text = b.office.call(&b.floor.id, Call::ListDesks).expect(case);
                assert_eq!(
                    text.matches(DESK).count(),
                    1,
                    "{case}: one desk line in {text}"
                );
                assert!(
                    !text.contains(&uuid(A)) && !text.contains(&uuid(B)),
                    "{case}: {text}"
                );
            },
        },
    ]
}

fn ended_ids_of(ended: &[EndedSession]) -> Vec<String> {
    ids(ended, |e| e.id.as_str())
}

#[test]
fn through_an_office_a_chat_desk_carries_its_workers_and_no_helper_is_a_desk() {
    let cases = [office_cases(), helper_cases_in_an_office()]
        .into_iter()
        .flatten();
    for (n, case) in cases.enumerate() {
        let b = building(&format!("join-{n}"), case.state);
        (case.check)(case.name, &b);
        let joined = super::super::joined(&b.office.lock(), &b.office.structured);
        invariant(case.name, &joined, &b.office.structured.tool_sessions());
        fs::remove_dir_all(&b.dir).expect("cleanup");
    }
}
