//! Hiring and talking to structured desks (structured/), and folding them into
//! the snapshot.

use super::{Inner, Office};
use crate::discovery::Discovered;
use crate::model::{
    ControlMode, EndedSession, HireRequest, ModelOption, Project, Session, SessionState, Spend,
};
use crate::structured::desks::{DeskInfo, Desks};
use crate::structured::{self, Spawn};
use std::path::Path;

/// The id that means "the tool's own default": pass no model or effort.
const DEFAULT: &str = "default";

mod join;

/// The snapshot's floors, sessions and signed-out desks, joined as the snapshot joins them
/// (spec 7): what discovery read of a chat desk's own conversation is on the desk.
pub(super) fn with_structured(
    found: &Discovered,
    desks: &Desks,
) -> (Vec<Project>, Vec<Session>, Vec<EndedSession>) {
    let j = join::join(found, desks, Vec::new());
    (j.projects, j.sessions, j.ended)
}

/// The snapshot's view of the building: discovery's finds and the chat desks joined, with
/// each floor's open calls and the stored calls in the queue between discovery's rows and
/// the chat desks' requests.
pub(super) fn joined(inner: &Inner, desks: &Desks) -> join::Joined {
    let calls = inner
        .files
        .values()
        .flat_map(|f| f.open.iter().cloned())
        .chain(inner.store.records.calls.iter().cloned())
        .collect();
    join::join(&inner.found, desks, calls)
}

impl Office {
    /// Start a structured desk on `project_id`'s floor (the checks a terminal
    /// hire makes have already passed).
    pub(super) fn spawn_structured(
        &self,
        project_id: &str,
        root: &Path,
        req: &HireRequest,
        model: &ModelOption,
        task: &str,
    ) -> Result<Session, String> {
        if !structured::supports(req.tool) {
            return Err("that tool can only run in a terminal".to_string());
        }
        let launch = self.setup.launch(req.tool, &self.home, None)?;
        let spawn = Spawn {
            tool: req.tool,
            cwd: root,
            model: (model.id != DEFAULT).then_some(model.id.as_str()),
            effort: (req.effort != DEFAULT).then_some(req.effort.as_str()),
            launch: &launch,
            task,
            resume: None,
        };
        let title = if task.is_empty() { "New task" } else { task };
        let info = DeskInfo {
            tool: req.tool,
            project_id: project_id.to_string(),
            root: root.to_path_buf(),
            title: title.to_string(),
            model: model.id.clone(),
            effort: req.effort.clone(),
        };
        let id = self.structured.start(info, &spawn)?;
        // A desk on a floor that is not standing yet puts it up.
        self.refresh();
        Ok(Session {
            session_name: None,
            session_name_source: None,
            self_reviewed: false,
            id,
            tool: req.tool,
            project_id: project_id.to_string(),
            title: title.to_string(),
            state: SessionState::Thinking,
            control: ControlMode::Structured,
            model: Some(model.id.clone()),
            effort: Some(req.effort.clone()),
            last_reply: None,
            last_activity_at: super::now_iso(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers: Vec::new(),
            pets: Vec::new(),
            worktree: false,
            branch: None,
        })
    }

    /// Reopen an ended structured desk's conversation, on the same desk.
    /// `None` when `session_id` is not one of them.
    pub(super) fn resume_structured(&self, session_id: &str) -> Option<Result<(), String>> {
        let (info, tool_session) = self.structured.resumable(session_id)?;
        let run = || {
            let launch = self.setup.launch(info.tool, &self.home, None)?;
            let spawn = Spawn {
                tool: info.tool,
                cwd: &info.root,
                model: (info.model != DEFAULT).then_some(info.model.as_str()),
                effort: (info.effort != DEFAULT).then_some(info.effort.as_str()),
                launch: &launch,
                task: "",
                resume: Some(&tool_session),
            };
            self.structured.resume(session_id, &spawn)?;
            self.refresh();
            Ok(())
        };
        Some(run())
    }

    pub fn structured_answer(
        &self,
        session_id: &str,
        approval_id: &str,
        choice_id: &str,
        values: &structured::Values,
    ) -> Result<(), String> {
        self.structured
            .answer(session_id, approval_id, choice_id, values)
    }

    pub fn structured_stop(&self, session_id: &str) -> Result<(), String> {
        self.structured.stop(session_id)
    }
}

#[cfg(test)]
mod tests;
