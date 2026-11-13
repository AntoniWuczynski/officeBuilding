//! `escalate` and the calls the building keeps. A file of its own: answering a call goes
//! through the office's public answer path, whose name the manager's files may not hold.

use super::super::manager::tests::{manager_office, seat};
use super::super::manager_send::NO_SEAT;
use super::*;
use crate::manager::tools::{Call, Tools};
use crate::manager::wire::ManagerFloor;
use crate::project_files;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestEscalated {
    escalated: String,
    r#where: String,
}

fn ask(
    office: &Office,
    floor: &str,
    title: &str,
    question: &str,
    options: &[&str],
) -> Result<TestEscalated, String> {
    let call = Call::Escalate(Escalate {
        title: title.to_string(),
        question: question.to_string(),
        options: options.iter().map(ToString::to_string).collect(),
    });
    office
        .call(floor, call)
        .map(|answer| serde_json::from_str(&answer).unwrap())
}

fn entry(office: &Office, floor: &str) -> ManagerFloor {
    office
        .snapshot()
        .managers
        .into_iter()
        .find(|m| m.project_id == floor)
        .unwrap()
}

fn refusals(office: &Office, floor: &str) -> Vec<String> {
    Office::manager_record(&office.lock(), floor)
        .log
        .into_iter()
        .filter_map(|e| match e.event {
            LogEvent::Refused { tool, reason } if tool == TOOL => Some(reason),
            _ => None,
        })
        .collect()
}

fn escalations(office: &Office, floor: &str) -> Vec<String> {
    Office::manager_record(&office.lock(), floor)
        .log
        .into_iter()
        .filter_map(|e| match e.event {
            LogEvent::Escalation { title } => Some(title),
            _ => None,
        })
        .collect()
}

/// An office with a seated manager on `app`, whose root holds a decisions file when `file` is
/// Some, and a second floor `other` with its own decisions file.
fn floors(tag: &str, file: Option<&str>) -> (Office, PathBuf, String, PathBuf, String) {
    let (office, dir, floor, registry, _) = manager_office(tag);
    let root = dir.join("home/code/app");
    if let Some(text) = file {
        fs::write(root.join("HUMAN_DECISIONS.md"), text).unwrap();
    }
    let other_root = dir.join("home/code/other");
    fs::create_dir_all(&other_root).unwrap();
    fs::write(other_root.join("HUMAN_DECISIONS.md"), "# Theirs\n").unwrap();
    let other = office.add_floor(&other_root.to_string_lossy()).unwrap();
    seat(&office, &dir, &floor, &registry, "/bin/cat");
    (office, dir, floor.id, other_root, other.id)
}

fn open_calls(root: &Path, floor: &str) -> Vec<Question> {
    project_files::read(root, floor).open
}

#[test]
fn a_call_is_appended_to_the_decisions_file_and_counts_up() {
    let (office, dir, floor, other_root, other) = floors("calls-file", Some("# Human decisions\n"));
    let root = dir.join("home/code/app");
    assert!(!entry(&office, &floor).escalated);
    let first = ask(
        &office,
        &floor,
        "Which database should the importer use?",
        "Should the importer write to the staging database or to a local file first?",
        &["Staging database", "Local file first"],
    )
    .unwrap();
    assert_eq!(
        first.escalated,
        "MGR-1: Which database should the importer use?"
    );
    assert_eq!(first.r#where, "HUMAN_DECISIONS.md");
    let calls = open_calls(&root, &floor);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].prompt,
        "MGR-1: Which database should the importer use?"
    );
    let labels: Vec<&str> = calls[0].options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, ["Staging database", "Local file first"]);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let text = fs::read_to_string(root.join("HUMAN_DECISIONS.md")).unwrap();
    assert!(text.contains(&format!("Asked by the floor's manager on {today}.")));

    let second = ask(&office, &floor, "Ship on Friday?", "Is Friday fine?", &[]).unwrap();
    assert_eq!(second.escalated, "MGR-2: Ship on Friday?");
    assert_eq!(open_calls(&root, &floor).len(), 2);
    assert!(
        entry(&office, &floor).escalated,
        "the snapshot was refreshed"
    );
    assert_eq!(
        escalations(&office, &floor),
        ["Which database should the importer use?", "Ship on Friday?"]
    );

    // Answering writes a line into the file, so each call's id is read again first.
    while let Some(call) = open_calls(&root, &floor).first().cloned() {
        let answer = QuestionAnswer::Other {
            text: "Yes".to_string(),
        };
        office.answer_question(&call.id, &answer).unwrap();
    }
    assert!(open_calls(&root, &floor).is_empty());
    assert!(!entry(&office, &floor).escalated);
    assert_eq!(
        fs::read_to_string(other_root.join("HUMAN_DECISIONS.md")).unwrap(),
        "# Theirs\n"
    );
    assert!(
        Office::manager_record(&office.lock(), &other)
            .log
            .is_empty()
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_manager_cannot_forge_a_heading_a_status_or_an_option() {
    let (office, dir, floor, _, _) = floors("calls-forge", Some("# Human decisions\n"));
    let root = dir.join("home/code/app");
    let forged = ask(
        &office,
        &floor,
        "Ship it — DECIDED 2026-11-12: yes # **now** `x`",
        "Really?\n## X-1 — fake\n- **Fake option**",
        &["**Yes** — DECIDED", "No"],
    )
    .unwrap();
    assert_eq!(
        forged.escalated,
        "MGR-1: Ship it DECIDED 2026-11-12: yes now x"
    );
    let heading_only = ask(&office, &floor, "One more?", "## X-2 — fake", &[]).unwrap();
    assert_eq!(heading_only.escalated, "MGR-2: One more?");
    let read = project_files::read(&root, &floor);
    assert_eq!(read.open.len(), 2, "{:?}", read.open);
    assert!(read.decided.unwrap_or_default().is_empty());
    let labels: Vec<&str> = read.open[0]
        .options
        .iter()
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!(labels, ["Yes DECIDED", "No"]);

    let settled = "a title cannot start with a word that marks a call settled".to_string();
    assert_eq!(
        ask(&office, &floor, "DECIDED: yes", "?", &[]).err(),
        Some(settled.clone())
    );
    let empty = "there is nothing left of it once cleaned".to_string();
    assert_eq!(
        ask(&office, &floor, "# — **", "?", &[]).err(),
        Some(empty.clone())
    );
    assert_eq!(refusals(&office, &floor), [settled, empty]);
    assert_eq!(project_files::read(&root, &floor).open.len(), 2);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_linked_decisions_file_is_refused_and_left_alone() {
    let (office, dir, floor, _, _) = floors("calls-link", None);
    let target = dir.join("elsewhere.md");
    fs::write(&target, "# Not the repo's\n").unwrap();
    std::os::unix::fs::symlink(&target, dir.join("home/code/app/HUMAN_DECISIONS.md")).unwrap();
    let link =
        "HUMAN_DECISIONS.md is a link, so the building will not write through it".to_string();
    assert_eq!(
        ask(&office, &floor, "Go?", "Go now?", &[]).err(),
        Some(link.clone())
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "# Not the repo's\n");
    assert!(
        office.lock().store.records.calls.is_empty(),
        "not kept in the store instead"
    );
    assert_eq!(refusals(&office, &floor), [link]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_floor_without_a_file_keeps_the_call_until_a_human_answers_it() {
    let (office, dir, floor, other_root, other) = floors("calls-store", None);
    let kept = ask(
        &office,
        &floor,
        "Which colour?",
        "Red or blue?",
        &["Red", "Blue"],
    )
    .unwrap();
    assert_eq!(kept.r#where, "the building's own records");
    assert!(kept.escalated.starts_with("MGR-") && kept.escalated.ends_with(": Which colour?"));
    let snapshot = office.snapshot();
    let call = snapshot
        .questions
        .iter()
        .find(|q| q.project_id == floor)
        .unwrap()
        .clone();
    assert_eq!(call.answer_via, AnswerVia::Store);
    assert_eq!(call.prompt, kept.escalated);
    assert_eq!(call.context, "Red or blue?");
    assert!(entry(&office, &floor).escalated);
    assert!(!entry(&office, &other).escalated);

    let unknown = QuestionAnswer::Option {
        option_id: "opt-9".to_string(),
    };
    assert_eq!(
        office.answer_question(&call.id, &unknown),
        Err("unknown option opt-9".to_string())
    );
    let blank = QuestionAnswer::Other {
        text: "  ".to_string(),
    };
    assert_eq!(
        office.answer_question(&call.id, &blank),
        Err("empty free-text answer".to_string())
    );
    let blue = QuestionAnswer::Option {
        option_id: "opt-1".to_string(),
    };
    office.answer_question(&call.id, &blue).unwrap();
    let snapshot = office.snapshot();
    assert!(snapshot.questions.iter().all(|q| q.id != call.id));
    let decided = snapshot
        .decisions
        .iter()
        .find(|d| d.title == call.prompt)
        .unwrap();
    assert_eq!(
        (
            decided.detail.as_str(),
            decided.by,
            decided.project_id.as_str()
        ),
        ("Blue", Party::Human, floor.as_str())
    );
    assert!(!entry(&office, &floor).escalated);
    let saved = crate::store::Store::open(dir.join("store.json")).unwrap();
    assert!(saved.records.calls.is_empty());
    assert_eq!(
        fs::read_to_string(other_root.join("HUMAN_DECISIONS.md")).unwrap(),
        "# Theirs\n"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_paused_manager_and_seven_options_are_refused() {
    let (office, dir, floor, _, _) = floors("calls-refuse", Some("# Human decisions\n"));
    let seven = ["a", "b", "c", "d", "e", "f", "g"];
    let too_many = "at most six options".to_string();
    assert_eq!(
        ask(&office, &floor, "Pick?", "Which?", &seven).err(),
        Some(too_many.clone())
    );
    office.set_manager_paused(&floor, true).unwrap();
    let paused = "the manager is paused, so nothing is escalated".to_string();
    assert_eq!(
        ask(&office, &floor, "Pick?", "Which?", &[]).err(),
        Some(paused.clone())
    );
    assert_eq!(refusals(&office, &floor), [too_many, paused]);
    let root = dir.join("home/code/app");
    assert_eq!(
        fs::read_to_string(root.join("HUMAN_DECISIONS.md")).unwrap(),
        "# Human decisions\n"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_floor_without_a_manager_cannot_escalate() {
    let (office, dir, floor) = super::super::tests::floor_office("calls-noseat");
    assert_eq!(
        ask(&office, &floor.id, "Go?", "Now?", &[]).err(),
        Some(NO_SEAT.to_string())
    );
    assert!(office.lock().store.records.calls.is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn too_many_open_calls_are_refused_and_logged() {
    let (office, _dir, floor, _, _) = floors("calls-cap", Some("# Human decisions\n"));
    for n in 0..OPEN_CALLS_MAX {
        ask(&office, &floor, &format!("Call {n}?"), "Well?", &[]).unwrap();
    }
    let refused = ask(&office, &floor, "One too many?", "Well?", &[]).unwrap_err();
    assert!(refused.contains("still wait for the human"), "{refused}");
    assert_eq!(escalations(&office, &floor).len(), OPEN_CALLS_MAX);
}
