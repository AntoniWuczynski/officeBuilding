//! The reconcile (spec 5.5): each desk's requests settled before the snapshot's desk state,
//! pending ids and queue are produced. For a Codex terminal, a hand is never raised without a
//! row, and a row never outlives its hand. A request is not a state with a time to compare:
//! it stays until the hooks or the rollout prove it over. For every other tool the hook feed
//! is applied as before, and only the handoff wording is added to its terminal rows.

use crate::discovery::{self, Discovered, codex::turns::Evidence};
use crate::feed::{Feed, requests::Asked};
use crate::focus::Host;
use crate::model::{AnswerVia, ControlMode, Handoff, Question, Session, SessionState, ToolKind};
use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

/// What a permission request's row says about it (C10): the hook proves only that it began.
const MAY_BE_SETTLED: &str = "The building saw a permission request begin. Codex, a hook or a \
                              reviewer may already have settled it, so look in its terminal.";
/// Added when a sub-agent asked.
const BY_A_HELPER: &str = " A helper of this session asked.";

/// Settle each desk's requests before the snapshot's desk state, pending ids and queue
/// are produced. For Codex: a hand is never raised without a row, and a row never
/// outlives its hand.
pub fn reconcile(
    found: &mut Discovered,
    feed: &mut Feed,
    host: impl FnMut(&str) -> Option<Host>,
    now: SystemTime,
) {
    let own: Vec<SessionState> = found.sessions.iter().map(|s| s.state).collect();
    feed.apply(&mut found.sessions);
    for (session, own) in found.sessions.iter_mut().zip(own) {
        if session.tool == ToolKind::Codex {
            settle_codex(
                session,
                own,
                &mut found.questions,
                found.codex.get(&session.id),
                feed,
            );
        }
    }
    found.unseen_prompts = found
        .sessions
        .iter()
        .filter(|s| {
            s.tool == ToolKind::Codex
                && s.control != ControlMode::Structured
                && feed.seen(&s.id).is_none()
                && feed.requests().open(&s.id).is_empty()
        })
        .map(|s| s.id.clone())
        .collect();
    found.unseen_prompts.sort();
    hand_off(&mut found.questions, &found.sessions, host);
    // As `assemble` orders them: most urgent first, then oldest ask first.
    found
        .questions
        .sort_by(|a, b| (a.priority, &a.asked_at).cmp(&(b.priority, &b.asked_at)));
    let live: BTreeSet<&str> = found.sessions.iter().map(|s| s.id.as_str()).collect();
    feed.requests_mut().forget_calls(|id| live.contains(id));
    feed.requests_mut().forget(|id, asked| {
        !live.contains(id)
            && now
                .duration_since(asked.at)
                .is_ok_and(|age| age > discovery::KEEP_FINISHED)
    });
}

/// Step 3 for one Codex session: `own` is its state as discovery made it, and its state now
/// is what the feed gave.
fn settle_codex(
    s: &mut Session,
    own: SessionState,
    questions: &mut Vec<Question>,
    evidence: Option<&Evidence>,
    feed: &mut Feed,
) {
    let seen = s.state;
    if let Some(ev) = evidence {
        let over: Vec<u64> = feed
            .requests()
            .open(&s.id)
            .iter()
            .filter(|a| proven_over(a, ev))
            .map(|a| a.seq)
            .collect();
        for seq in over {
            feed.requests_mut().retire(&s.id, seq);
        }
    }
    let rows: Vec<Question> = feed
        .requests()
        .open(&s.id)
        .iter()
        .map(|a| request_row(s, a))
        .collect();
    let mine = |q: &Question| q.session_id == s.id;
    let ids = |prose: bool| -> Vec<String> {
        questions
            .iter()
            .filter(|q| mine(q) && discovery::is_prose_ask(&q.id) == prose)
            .map(|q| q.id.clone())
            .collect()
    };
    let mut formal = ids(false);
    formal.extend(rows.iter().map(|q| q.id.clone()));
    let prose = ids(true);
    // A prose ask outlives the `Stop` of the turn that asked it, and goes when work resumes.
    let keep_prose = formal.is_empty()
        && own == SessionState::WaitingHuman
        && !prose.is_empty()
        && !matches!(
            seen,
            SessionState::Running | SessionState::Thinking | SessionState::Error
        );
    questions.retain(|q| !mine(q) || !discovery::is_prose_ask(&q.id) || keep_prose);
    questions.extend(rows);
    if !formal.is_empty() {
        s.state = SessionState::WaitingHuman;
        s.pending_question_ids = formal;
    } else if keep_prose {
        s.state = SessionState::WaitingHuman;
        s.pending_question_ids = prose;
    } else {
        s.pending_question_ids.clear();
    }
}

/// The rollout proves the request over: its root turn ended, or its call has its output.
fn proven_over(a: &Asked, ev: &Evidence) -> bool {
    let turn_ended = a.agent_id.is_none()
        && a.turn_id
            .as_ref()
            .is_some_and(|t| ev.ended_turns.contains(t));
    let call_done = a
        .call_id
        .as_ref()
        .is_some_and(|c| ev.done_calls.contains(c));
    turn_ended || call_done
}

/// The queue row of a permission request the hooks saw begin.
fn request_row(s: &Session, a: &Asked) -> Question {
    let prompt = if a.detail.is_empty() {
        format!("Codex may be waiting for your approval to use {}", a.tool)
    } else {
        format!("Codex may be waiting for your approval: {}", a.detail)
    };
    let mut context = MAY_BE_SETTLED.to_string();
    if a.agent_id.is_some() {
        context.push_str(BY_A_HELPER);
    }
    Question {
        id: format!("{}:approval:{}", s.id, a.seq),
        project_id: s.project_id.clone(),
        session_id: s.id.clone(),
        prompt,
        options: Vec::new(),
        allow_other: false,
        answer_via: AnswerVia::Terminal,
        priority: 0,
        asked_at: crate::structured::desks::iso(a.at),
        context,
        source_file: None,
        handoff: None,
    }
}

/// Step 5: how to reach each row answered in a terminal, by its session's launch mode.
/// `host` is asked once per session at most, and only for a session with such a row.
fn hand_off(
    questions: &mut [Question],
    sessions: &[Session],
    mut host: impl FnMut(&str) -> Option<Host>,
) {
    let mut hosts: BTreeMap<String, Option<Host>> = BTreeMap::new();
    for q in questions
        .iter_mut()
        .filter(|q| q.answer_via == AnswerVia::Terminal)
    {
        let control = sessions
            .iter()
            .find(|s| s.id == q.session_id)
            .map(|s| s.control);
        let handoff = match control {
            Some(ControlMode::Full) => Handoff::Docked,
            Some(ControlMode::ReadOnly) => Handoff::ReadOnly,
            Some(ControlMode::RaiseWindow) => {
                let found = *hosts
                    .entry(q.session_id.clone())
                    .or_insert_with(|| host(&q.session_id));
                match found {
                    Some(Host::Window { app, tab }) => Handoff::Window {
                        app: app.to_string(),
                        tab,
                    },
                    Some(Host::Background) => Handoff::Docked,
                    Some(Host::Nowhere) | None => Handoff::Elsewhere,
                }
            }
            // A chat desk's rows are answered in its conversation, not here.
            Some(ControlMode::Structured) | None => Handoff::Elsewhere,
        };
        q.handoff = Some(handoff);
    }
}

#[cfg(test)]
mod tests;
