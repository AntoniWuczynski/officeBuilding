//! A take-over: a new session A Human asked for, started beside an old one in the same folder
//! and pointed at a brief of the old one's conversation. The old session is left exactly as it
//! is, and the handover's book gets nothing but the brief's number.

use super::super::launch::Launched;
use super::run::{Prepared, Receiver, Talk, without_tail};
use super::{FOLDER_GONE, NOT_SET_UP, Office, desk, now};
use crate::handover::view::{TakeOver, TakeOverStart};
use crate::handover::{Tool, brief, start};
use crate::model::ControlMode;
use crate::office::Inner;
use crate::settings::StartWith;
use std::path::PathBuf;

/// Take-overs listed in the view.
const KEPT: usize = 50;

/// Everything a take-over needs, gathered under the lock.
pub(super) struct TakeOverPlan {
    pub(super) session_id: String,
    /// The floor the old session is on now, after any move.
    project_id: String,
    pub(super) from: Tool,
    pub(super) to: Tool,
    start: StartWith,
    title: String,
    pub(super) folder: PathBuf,
    floor: String,
    branch: Option<String>,
    worktree: bool,
}

impl TakeOverPlan {
    fn receiver(&self) -> Receiver<'_> {
        Receiver {
            from: self.from,
            to: self.to,
            from_session: &self.session_id,
            project_id: &self.project_id,
            folder: &self.folder,
            start: &self.start,
            title: &self.title,
            floor: &self.floor,
            branch: self.branch.as_deref(),
            worktree: self.worktree,
            reason: brief::Reason::Asked,
        }
    }
}

impl Office {
    /// A Human asked for a new session to take over from `session_id`, started with `start`
    /// in the same folder. Nothing at the old session is touched. Err: why not, in words.
    pub fn take_over_desk(&self, session_id: &str, start: &TakeOverStart) -> Result<(), String> {
        let plan = {
            let mut guard = self.lock();
            let plan = self.take_over_plan(&guard, session_id, start)?;
            guard.handover.taking_over.insert(plan.session_id.clone());
            plan
        };
        let result = self.take_over_marked(&plan);
        self.lock().handover.taking_over.remove(&plan.session_id);
        result
    }

    fn take_over_marked(&self, plan: &TakeOverPlan) -> Result<(), String> {
        let Some(host) = &self.handover_host else {
            return Err(NOT_SET_UP.to_string());
        };
        let talk = self.conversation_to_take_over(plan)?;
        let id = self.brief_id(host);
        let prepared = self.prepare(host, &plan.receiver(), &talk, &id)?;
        self.start_beside(plan, id, prepared)
    }

    /// Who may be taken over, checked in order, or the refusal in words.
    pub(super) fn take_over_plan(
        &self,
        inner: &Inner,
        session_id: &str,
        start: &TakeOverStart,
    ) -> Result<TakeOverPlan, String> {
        if self.handover_host.is_none() {
            return Err(NOT_SET_UP.to_string());
        }
        if !start::plain_session_id(session_id) {
            return Err("that session's id cannot be written into a prompt".to_string());
        }
        let session = inner.found.sessions.iter().find(|s| s.id == session_id);
        if session.is_some_and(|s| s.control == ControlMode::Structured)
            || self
                .structured
                .sessions()
                .iter()
                .any(|s| s.id == session_id)
        {
            return Err("a desk in a tool's chat view cannot be taken over yet".to_string());
        }
        let session = session.ok_or("that session is no longer at a desk")?;
        let from = Tool::of(session.tool)
            .ok_or("only a Claude Code or Codex session can be taken over")?;
        if Self::staff_sessions(inner).iter().any(|s| s == session_id) {
            return Err("a floor's manager or QA reviewer is not taken over".to_string());
        }
        if inner.handover.underway.contains(session_id)
            || inner.handover.taking_over.contains(session_id)
        {
            return Err("this desk is being handed over".to_string());
        }
        if inner
            .owned
            .iter()
            .any(|(t, s)| s == session_id && Self::being_ended(inner, t))
        {
            return Err("a manager is ending this desk now".to_string());
        }
        let folder = Self::session_folder(inner, session).ok_or(FOLDER_GONE)?;
        Ok(TakeOverPlan {
            session_id: session_id.to_string(),
            project_id: session.project_id.clone(),
            from,
            to: start.tool,
            start: StartWith::Chosen {
                model: start.model.clone(),
                effort: start.effort.clone(),
            },
            title: session.title.clone(),
            folder,
            floor: Self::floor_name(inner, &session.project_id),
            branch: session.branch.clone(),
            worktree: session.worktree,
        })
    }

    /// The old session's visible conversation, read with no lock held. Err: the building
    /// cannot read any of it, so there is nothing to take over.
    pub(super) fn conversation_to_take_over(&self, plan: &TakeOverPlan) -> Result<Talk, String> {
        let (entries, earlier) = desk::conversation(plan.from, &self.home, &plan.session_id);
        if entries.is_empty() && !earlier {
            return Err(
                "the building cannot read that session's conversation, so there is nothing to \
                 take over"
                    .to_string(),
            );
        }
        Ok((entries, earlier))
    }

    /// Start the new session in the old one's folder and list the take-over. A start that
    /// fails removes the brief and lists nothing.
    pub(super) fn start_beside(
        &self,
        plan: &TakeOverPlan,
        id: String,
        prepared: Prepared,
    ) -> Result<(), String> {
        let Prepared {
            command,
            brief: path,
            route,
        } = prepared;
        // `launch_routed` closes the route itself when the launch fails.
        let terminal = self
            .launch_routed(
                &plan.folder,
                &command,
                Launched::Discovered {
                    title: format!("Take over from {}", plan.session_id),
                },
                route,
            )
            .map_err(|why| {
                brief::remove(&path);
                format!("{} did not start: {}", plan.to.label(), without_tail(&why))
            })?;
        self.note_view(plan.to.kind(), &terminal);
        let mut guard = self.lock();
        let inner = &mut *guard;
        let listed = &mut inner.handover.take_overs;
        listed.insert(
            0,
            (
                TakeOver {
                    id,
                    from_session: plan.session_id.clone(),
                    from: plan.from,
                    to: plan.to,
                    to_session: None,
                    at: now(),
                },
                terminal,
            ),
        );
        listed.truncate(KEPT);
        self.resurvey(inner);
        Ok(())
    }
}
