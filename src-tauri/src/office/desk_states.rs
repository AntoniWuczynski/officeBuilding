//! Each desk's state for the alert tracker (alerts/): the same desks the snapshot shows.

use super::{Inner, Office, with_structured};
use crate::alerts::DeskState;
use crate::discovery::Discovered;
use crate::model::{ControlMode, Project, Session, SessionState};
use std::time::SystemTime;

impl Office {
    /// Every desk's state now, for the alert tracker: discovery's sessions and the structured desks.
    #[must_use]
    pub fn desk_states(&self) -> Vec<DeskState> {
        let inner = self.lock();
        let (projects, sessions, _) = with_structured(&inner.found, &self.structured);
        states(&projects, sessions, |id| inner.feed.reports(id))
    }
}

impl Inner {
    /// Collect the hook feed, then settle every desk's requests into `found`.
    pub(super) fn settle(&mut self, found: &mut Discovered, now: SystemTime) {
        self.feed.collect(now);
        crate::attention::reconcile(found, &mut self.feed, |id| self.discovery.host_of(id), now);
    }
}

/// The state the alert tracker judges: a chat desk with a request open needs A Human
/// even while it works on.
fn alert_state(session: &Session) -> SessionState {
    let asks = session.control == ControlMode::Structured
        && !session.pending_question_ids.is_empty()
        && matches!(
            session.state,
            SessionState::Running | SessionState::Thinking
        );
    if asks {
        SessionState::WaitingHuman
    } else {
        session.state
    }
}

/// One entry per desk. Helpers (sub-agents) are not desks and never alert.
fn states(
    projects: &[Project],
    sessions: Vec<Session>,
    reports: impl Fn(&str) -> bool,
) -> Vec<DeskState> {
    sessions
        .into_iter()
        .map(|session| DeskState {
            hooked: session.control == ControlMode::Structured || reports(&session.id),
            floor: projects
                .iter()
                .find(|p| p.id == session.project_id)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            state: alert_state(&session),
            id: session.id,
            title: session.title,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Helper, Spend, ToolKind};

    fn session(id: &str, project_id: &str, control: ControlMode) -> Session {
        Session {
            session_name: None,
            session_name_source: None,
            self_reviewed: false,
            id: id.to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: project_id.to_string(),
            title: format!("title {id}"),
            state: SessionState::Running,
            control,
            model: None,
            effort: None,
            last_reply: None,
            last_activity_at: "2000-01-01T00:00:00Z".to_string(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers: Vec::new(),
            pets: Vec::new(),
            worktree: false,
            branch: None,
        }
    }

    fn project(id: &str, name: &str) -> Project {
        Project {
            id: id.to_string(),
            name: name.to_string(),
            path: format!("~/{name}"),
            session_ids: Vec::new(),
        }
    }

    #[test]
    fn the_floor_is_the_sessions_project_name_or_empty() {
        let out = states(
            &[project("p1", "garden")],
            vec![
                session("a", "p1", ControlMode::Full),
                session("b", "nowhere", ControlMode::Full),
            ],
            |_| false,
        );
        assert_eq!(out[0].floor, "garden");
        assert_eq!(out[0].title, "title a");
        assert_eq!(out[0].state, SessionState::Running);
        assert_eq!(out[1].floor, "");
    }

    #[test]
    fn structured_and_reported_desks_are_hooked_and_others_are_not() {
        let out = states(
            &[],
            vec![
                session("structured", "p", ControlMode::Structured),
                session("reported", "p", ControlMode::Full),
                session("sampled", "p", ControlMode::ReadOnly),
            ],
            |id| id == "reported",
        );
        let hooked: Vec<(&str, bool)> = out.iter().map(|d| (d.id.as_str(), d.hooked)).collect();
        assert_eq!(
            hooked,
            [("structured", true), ("reported", true), ("sampled", false)]
        );
    }

    #[test]
    fn helpers_add_no_entries() {
        let mut parent = session("parent", "p", ControlMode::Full);
        parent.helpers.push(Helper {
            id: "helper".to_string(),
            state: SessionState::Running,
            worktree: false,
            branch: None,
            model: None,
            effort: None,
        });
        let out = states(&[], vec![parent], |_| false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "parent");
    }
}
