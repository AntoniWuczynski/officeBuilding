//! Each desk's state for the alert tracker (alerts/): the same desks the snapshot shows.

use super::{Office, with_structured};
use crate::alerts::DeskState;
use crate::model::{ControlMode, Project, Session};

impl Office {
    /// Every desk's state now, for the alert tracker: discovery's sessions and the structured desks.
    #[must_use]
    pub fn desk_states(&self) -> Vec<DeskState> {
        let inner = self.lock();
        let (projects, sessions, _) = with_structured(&inner.found, &self.structured);
        states(&projects, sessions, |id| inner.feed.reports(id))
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
            id: session.id,
            title: session.title,
            state: session.state,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Helper, SessionState, Spend, ToolKind};

    fn session(id: &str, project_id: &str, control: ControlMode) -> Session {
        Session {
            id: id.to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: project_id.to_string(),
            title: format!("title {id}"),
            state: SessionState::Running,
            control,
            model: None,
            effort: None,
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
        });
        let out = states(&[], vec![parent], |_| false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "parent");
    }
}
