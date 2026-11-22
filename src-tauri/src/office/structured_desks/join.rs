//! The join (M2b spec 7): discovery's finds and the chat desks, joined by Codex's thread id
//! before anything is deduplicated. What discovery read of a thread a chat desk owns (its
//! workers, its pets, a plain-prose ask) lands on the desk. Only then are live desks, queue
//! rows, signed-out desks and unseen prompts made one per id.

use crate::discovery::{self, Discovered};
use crate::model::{
    AnswerVia, EndedSession, Handoff, Helper, Project, Question, Session, SessionState,
};
use crate::structured::desks::Desks;
use std::collections::{BTreeMap, BTreeSet};

/// The building as the snapshot shows it: discovery's finds and the chat desks, joined by
/// Codex's thread id before anything is deduplicated.
pub(in crate::office) struct Joined {
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
    pub ended: Vec<EndedSession>,
    pub questions: Vec<Question>,
    pub unseen_prompts: Vec<String>,
}

/// `calls`: the rows that are neither discovery's nor a chat desk's (each floor's open
/// calls, the stored calls). They keep their place in the queue.
pub(in crate::office) fn join(found: &Discovered, desks: &Desks, calls: Vec<Question>) -> Joined {
    // Step 1: observe.
    let threads: BTreeMap<String, String> = desks.threads().into_iter().collect();
    let running: BTreeSet<&str> = threads.values().map(String::as_str).collect();
    let mut ours = desks.sessions();

    // Step 2: project each running desk's thread onto the desk. Moved prose rows are kept
    // by the id of the row they replace.
    let mut moved: BTreeMap<String, Question> = BTreeMap::new();
    for desk in &mut ours {
        let Some(thread) = threads.get(&desk.id) else {
            continue;
        };
        let seen = found.sessions.iter().find(|s| &s.id == thread);
        let disk = seen.map_or(&[][..], |n| n.helpers.as_slice());
        desk.helpers = merged(
            disk,
            std::mem::take(&mut desk.helpers),
            found.codex_gone.get(thread),
        );
        if let Some(n) = seen {
            desk.pets.clone_from(&n.pets);
        }
        let prose = found
            .questions
            .iter()
            .find(|q| &q.session_id == thread && discovery::is_prose_ask(&q.id));
        if desk.state == SessionState::Idle
            && let Some(row) = prose
        {
            let id = discovery::prose_row_id(&desk.id);
            desk.pending_question_ids.push(id.clone());
            let row_on_desk = Question {
                id,
                project_id: desk.project_id.clone(),
                session_id: desk.id.clone(),
                answer_via: AnswerVia::Chat,
                handoff: Some(Handoff::Conversation),
                allow_other: false,
                ..row.clone()
            };
            moved.insert(row.id.clone(), row_on_desk);
        }
    }

    // Step 3: deduplicate.
    let mut sessions: Vec<Session> = found
        .sessions
        .iter()
        .filter(|s| !running.contains(s.id.as_str()))
        .cloned()
        .collect();
    let projects = found
        .projects
        .iter()
        .map(|p| {
            let mut p = p.clone();
            p.session_ids.retain(|id| !running.contains(id.as_str()));
            p.session_ids.extend(
                ours.iter()
                    .filter(|s| s.project_id == p.id)
                    .map(|s| s.id.clone()),
            );
            p
        })
        .collect();
    sessions.extend(ours);

    let mut questions: Vec<Question> = found
        .questions
        .iter()
        .filter_map(|q| match moved.remove(&q.id) {
            Some(on_desk) => Some(on_desk),
            None => (!running.contains(q.session_id.as_str())).then(|| q.clone()),
        })
        .collect();
    questions.extend(calls);
    questions.extend(desks.questions());

    let ended = sheet(found, desks, &running);
    let unseen_prompts = found
        .unseen_prompts
        .iter()
        .filter(|id| !running.contains(id.as_str()))
        .cloned()
        .collect();

    Joined {
        projects,
        sessions,
        ended,
        questions,
        unseen_prompts,
    }
}

/// The sign-out sheet. An ended desk whose thread runs on in a terminal is that terminal's
/// desk now (D14), and any desk's thread, running or ended, is on the sheet only as the desk.
fn sheet(found: &Discovered, desks: &Desks, running: &BTreeSet<&str>) -> Vec<EndedSession> {
    let live: BTreeSet<&str> = found.sessions.iter().map(|s| s.id.as_str()).collect();
    let mut ended: Vec<EndedSession> = desks
        .ended()
        .into_iter()
        .filter(|e| {
            !desks
                .resumable(&e.id)
                .is_some_and(|(_, thread)| live.contains(thread.as_str()))
        })
        .collect();
    let gone = desks.ended_tool_sessions();
    ended.extend(
        found
            .ended
            .iter()
            .filter(|e| !running.contains(e.id.as_str()) && !gone.contains(&e.id))
            .cloned(),
    );
    ended.sort_by(|a, b| b.ended_at.cmp(&a.ended_at));
    ended
}

/// A chat desk's helpers: the disk's, in its order, then the driver's the disk has not read.
/// The disk is the authority on a worker it has read (`read`), except that the driver knows
/// when one is asking (D7, D11). A worker the disk proves over (`gone`) is dropped from the
/// driver's list, and one the disk only stopped drawing for want of fresh evidence is kept.
fn merged(disk: &[Helper], driver: Vec<Helper>, gone: Option<&BTreeSet<String>>) -> Vec<Helper> {
    let mut helpers = disk.to_vec();
    for h in driver {
        let asking = h.state == SessionState::WaitingHuman;
        if let Some(kept) = helpers.iter_mut().find(|k| k.id == h.id) {
            if asking {
                kept.state = SessionState::WaitingHuman;
            }
        } else if asking || !gone.is_some_and(|ids| ids.contains(&h.id)) {
            helpers.push(h);
        }
    }
    helpers
}

#[cfg(test)]
mod tests;
