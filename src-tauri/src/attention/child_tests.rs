//! A worker's permission request on its root's desk (spec 5.3, 10.3). Hook files are composed
//! from G18 (the hook carries the root's `session_id` and the worker's thread id as
//! `agent_id`), placeholder ids and commands only.

use super::tests::{P, S, Walk, assert_hand_matches_rows, at, codex, hooked, rows_of, seen};
use super::*;
use crate::model::Helper;
use crate::setup::HookEvent;

const ADA: &str = "0199c0de-0000-4000-8000-000000000002";
const LIN: &str = "0199c0de-0000-4000-8000-000000000003";
const STRANGER: &str = "0199c0de-0000-4000-8000-000000000009";

fn helper(id: &str, state: SessionState) -> Helper {
    Helper {
        id: id.to_string(),
        state,
        worktree: false,
        branch: None,
        model: None,
        effort: None,
    }
}

/// The root with two running workers, and what the disk read of them.
fn team() -> Discovered {
    let mut found = codex(SessionState::Running, Vec::new());
    found.sessions[0].helpers = vec![
        helper(ADA, SessionState::Running),
        helper(LIN, SessionState::Running),
    ];
    for (id, name) in [(ADA, "Ada"), (LIN, "Lin")] {
        found
            .helper_names
            .insert(discovery::helper_key(S, id), name.to_string());
    }
    found.codex_gone.insert(
        S.to_string(),
        BTreeSet::from([ADA.to_string(), LIN.to_string()]),
    );
    found
}

/// A `PreToolUse` and a `PermissionRequest` of call `c1` in turn `t1`, from `agent` (the root
/// itself when empty).
fn ask(w: &Walk, agent: &str, cmd: &str, from: u64) {
    let who = if agent.is_empty() {
        String::new()
    } else {
        format!(r#","agent_id":"{agent}""#)
    };
    let tool = format!(r#","tool_name":"shell","tool_input":{{"command":"{cmd}"}}"#);
    w.hook(
        HookEvent::PreToolUse,
        &hooked(S, "t1", &format!(r#"{who},"tool_use_id":"c1"{tool}"#)),
        at(from),
    );
    w.hook(
        HookEvent::PermissionRequest,
        &hooked(S, "t1", &format!("{who}{tool}")),
        at(from + 1),
    );
}

fn evidence(ended: &[&str], done: &[&str]) -> Evidence {
    Evidence {
        turn_id: None,
        ended_turns: ended.iter().map(ToString::to_string).collect(),
        done_calls: done.iter().map(ToString::to_string).collect(),
    }
}

fn helper_state(found: &Discovered, id: &str) -> SessionState {
    found.sessions[0]
        .helpers
        .iter()
        .find(|h| h.id == id)
        .map(|h| h.state)
        .expect("the helper")
}

/// The scan after one request of `agent`, with the disk as `disk` leaves it.
fn one_request(agent: &str, disk: impl FnOnce(&mut Discovered)) -> (Walk, Discovered) {
    let mut w = Walk::new();
    let mut found = team();
    disk(&mut found);
    ask(&w, agent, "tool-x --flag", 0);
    let out = w.scan(&found, at(2));
    (w, out)
}

fn one_row_by(out: &Discovered, name: Option<&str>) {
    let [q] = out.questions.as_slice() else {
        panic!("one row, not {}", out.questions.len());
    };
    assert_eq!(q.session_id, S);
    assert_eq!(q.asked_by.as_deref(), name);
    assert_eq!(q.context, MAY_BE_SETTLED);
    assert_eq!(q.id, format!("{S}:approval:1"));
    assert_eq!(seen(out, S).0, SessionState::WaitingHuman);
}

#[test]
fn a_worker_that_asks_is_named_on_its_roots_row_and_waits_at_its_desk() {
    let (_, out) = one_request(ADA, |_| {});
    one_row_by(&out, Some("Ada"));
    assert_eq!(helper_state(&out, ADA), SessionState::WaitingHuman, "1");
    assert_eq!(
        helper_state(&out, LIN),
        SessionState::Running,
        "1 the other"
    );
}

#[test]
fn a_worker_request_goes_when_the_disk_proves_it_over() {
    type Case = (&'static str, fn(&mut Discovered));
    let cases: [Case; 4] = [
        ("2 its turn ended", |f| {
            f.codex.insert(ADA.to_string(), evidence(&["t1"], &[]));
        }),
        ("3 its call has output", |f| {
            f.codex.insert(ADA.to_string(), evidence(&[], &["c1"]));
        }),
        ("3 the root's evidence has the call", |f| {
            f.codex.insert(S.to_string(), evidence(&[], &["c1"]));
        }),
        ("4 the worker left", |f| {
            f.sessions[0].helpers.retain(|h| h.id != ADA);
        }),
    ];
    for (name, disk) in cases {
        let (w, out) = one_request(ADA, disk);
        assert!(out.questions.is_empty(), "{name}: no row");
        assert!(w.feed.requests().open(S).is_empty(), "{name}: retired");
        assert_eq!(seen(&out, S).0, SessionState::Running, "{name}: hand down");
        if let Some(h) = out.sessions[0].helpers.iter().find(|h| h.id == ADA) {
            assert_eq!(
                h.state,
                SessionState::Running,
                "{name}: as discovery gave it"
            );
        }
    }
}

#[test]
fn a_worker_not_drawn_but_not_proven_over_keeps_its_request() {
    let (_, out) = one_request(ADA, |f| {
        f.sessions[0].helpers.retain(|h| h.id != ADA);
        f.codex_gone.clear();
    });
    one_row_by(&out, Some("Ada"));
}

#[test]
fn a_worker_the_disk_never_read_is_named_by_its_id_and_changes_no_helper() {
    let (_, out) = one_request(STRANGER, |f| {
        f.helper_names.clear();
        f.codex_gone.clear();
    });
    one_row_by(&out, Some("helper 0199c0de"));
    for id in [ADA, LIN] {
        assert_eq!(helper_state(&out, id), SessionState::Running, "5 {id}");
    }
}

#[test]
fn a_request_with_no_agent_id_has_no_asker() {
    let (_, out) = one_request("", |_| {});
    one_row_by(&out, None);
    for id in [ADA, LIN] {
        assert_eq!(helper_state(&out, id), SessionState::Running, "6 {id}");
    }
}

#[test]
fn two_requests_are_two_rows_on_one_desk() {
    let mut w = Walk::new();
    ask(&w, "", "tool-x --flag", 0);
    w.hook(
        HookEvent::PermissionRequest,
        &hooked(
            S,
            "t1",
            &format!(
                r#","agent_id":"{LIN}","tool_name":"shell","tool_input":{{"command":"tool-y"}}"#
            ),
        ),
        at(3),
    );
    let out = w.scan(&team(), at(4));
    let by: Vec<Option<&str>> = out
        .questions
        .iter()
        .map(|q| q.asked_by.as_deref())
        .collect();
    assert_eq!(by, [None, Some("Lin")], "7");
    assert_eq!(rows_of(&out, S).len(), 2, "7 both on the root");
    assert_eq!(
        out.sessions.len(),
        1,
        "7 one desk, so one alerting entry, none for the worker"
    );
    assert_eq!(out.sessions[0].project_id, P);
    assert_eq!(helper_state(&out, LIN), SessionState::WaitingHuman, "7");
    assert_eq!(helper_state(&out, ADA), SessionState::Running, "7");
}

#[test]
fn the_hand_matches_the_rows_in_every_child_case() {
    for (name, agent) in [("worker", ADA), ("stranger", STRANGER), ("root", "")] {
        let (_, out) = one_request(agent, |_| {});
        assert_hand_matches_rows(name, &out);
    }
}
