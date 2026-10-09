//! Hiring and talking to structured desks (structured/), and folding them into
//! the snapshot.

use super::Office;
use crate::discovery::Discovered;
use crate::model::{
    ControlMode, EndedSession, HireRequest, ModelOption, Project, Session, SessionState, Spend,
};
use crate::structured::desks::{Backlog, DeskInfo, Desks};
use crate::structured::{self, Spawn};
use std::path::Path;

/// The id that means "the tool's own default": pass no model or effort.
const DEFAULT: &str = "default";

/// The snapshot's floors and sessions: discovery's, with each running
/// structured desk added to its floor. A conversation discovery also found in
/// the tool's session files (Codex writes one for every app-server thread) is
/// the structured desk's own and gets no desk of its own.
pub(super) fn with_structured(
    found: &Discovered,
    desks: &Desks,
) -> (Vec<Project>, Vec<Session>, Vec<EndedSession>) {
    let theirs = desks.tool_sessions();
    let ours = desks.sessions();
    let mut sessions: Vec<Session> = found
        .sessions
        .iter()
        .filter(|s| !theirs.contains(&s.id))
        .cloned()
        .collect();
    let projects = found
        .projects
        .iter()
        .map(|p| {
            let mut p = p.clone();
            p.session_ids.retain(|id| !theirs.contains(id));
            p.session_ids.extend(
                ours.iter()
                    .filter(|s| s.project_id == p.id)
                    .map(|s| s.id.clone()),
            );
            p
        })
        .collect();
    sessions.extend(ours);
    // An ended desk's conversation is on the sheet once, as the desk: resuming
    // it there reopens the conversation in a conversation panel.
    let gone = desks.ended_tool_sessions();
    let mut ended: Vec<EndedSession> = desks.ended();
    ended.extend(
        found
            .ended
            .iter()
            .filter(|e| !gone.contains(&e.id))
            .cloned(),
    );
    ended.sort_by(|a, b| b.ended_at.cmp(&a.ended_at));
    (projects, sessions, ended)
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
            id,
            tool: req.tool,
            project_id: project_id.to_string(),
            title: title.to_string(),
            state: SessionState::Thinking,
            control: ControlMode::Structured,
            model: Some(model.id.clone()),
            effort: Some(req.effort.clone()),
            last_activity_at: super::now_iso(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers: Vec::new(),
            worktree: false,
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

    pub fn structured_backlog(&self, session_id: &str) -> Result<Backlog, String> {
        self.structured.backlog(session_id)
    }

    pub fn structured_prompt(&self, session_id: &str, text: &str) -> Result<(), String> {
        self.structured.prompt(session_id, text)
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

    pub fn structured_cancel(&self, session_id: &str) -> Result<(), String> {
        self.structured.cancel(session_id)
    }

    pub fn structured_stop(&self, session_id: &str) -> Result<(), String> {
        self.structured.stop(session_id)
    }
}
