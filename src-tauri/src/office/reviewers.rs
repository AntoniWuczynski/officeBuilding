//! The QA reviewer's desk: the building starts one per floor, and ends it once it has
//! reported, left or run out of time. Ending it is the one place outside a demolition where
//! the building stops an agent (A Human, 2026-11-12), and it can reach only a terminal
//! `seat_reviewer` registered: never a desk, a shell or a structured agent.

use super::launch::Launched;
use super::staff::{PermissionMode, Role};
use super::{Inner, Office, STOP_WAIT};
use crate::discovery::{self, Discovered};
use crate::model::ToolKind;
use crate::qa::brief::Job;
use crate::settings::ReviewerTool;
use std::path::Path;
use std::time::Duration;

impl Office {
    /// Start the QA reviewer for `job` on a floor, as a terminal desk. One per floor.
    pub fn hire_reviewer(&self, project_id: &str, job: &Job) -> Result<String, String> {
        // Checked first so a refused hire writes nothing; `seat_reviewer` checks again.
        Self::seat_free(&self.lock(), project_id)?;
        let (tool, model, effort) =
            self.settings
                .as_ref()
                .map_or((ReviewerTool::ClaudeCode, None, None), |settings| {
                    let qa = settings.get().qa;
                    (qa.tool, qa.model, qa.effort)
                });
        let tool = match tool {
            ReviewerTool::ClaudeCode => ToolKind::ClaudeCode,
        };
        let role = Role {
            name: format!("qa-{project_id}"),
            instructions: job.instructions.clone(),
            permissions: PermissionMode::ReadOnly,
            folder: job.folder.clone(),
            report: Some(job.report.clone()),
            helper: None,
        };
        let command =
            self.staff_launch(&role, tool, model.as_deref(), effort.as_deref(), &job.task)?;
        self.seat_reviewer(project_id, &job.folder, &command, &job.title)
    }

    /// The floor exists and has no reviewer yet.
    fn seat_free(inner: &Inner, project_id: &str) -> Result<(), String> {
        Self::root_key(inner, project_id)?;
        if inner.reviewers.values().any(|floor| floor == project_id) {
            return Err("a review is already running on this floor".to_string());
        }
        Ok(())
    }

    /// Run `command` in `folder` as the floor's reviewer. The only place a terminal becomes
    /// a reviewer's, so the only one `dismiss_reviewer` will ever end.
    fn seat_reviewer(
        &self,
        project_id: &str,
        folder: &Path,
        command: &str,
        title: &str,
    ) -> Result<String, String> {
        Self::seat_free(&self.lock(), project_id)?;
        let terminal = self.launch(
            folder,
            command,
            Launched::Discovered {
                title: title.to_string(),
            },
        )?;
        self.lock()
            .reviewers
            .insert(terminal.clone(), project_id.to_string());
        Ok(terminal)
    }

    /// Whether a reviewer's terminal has exited (or is not a reviewer's).
    #[must_use]
    pub fn reviewer_left(&self, terminal: &str) -> bool {
        if !self.lock().reviewers.contains_key(terminal) {
            return true;
        }
        !matches!(self.terminals.wait_exit(terminal, Duration::ZERO), Ok(None))
    }

    /// End a reviewer the building started and release its terminal. The one place the
    /// building stops an agent outside a demolition (A Human, 2026-11-12), and it refuses
    /// any terminal that is not a reviewer's.
    pub fn dismiss_reviewer(&self, terminal: &str) -> Result<(), String> {
        {
            let mut inner = self.lock();
            if inner.reviewers.remove(terminal).is_none() {
                return Err("that terminal is not a QA review".to_string());
            }
            // Ending a reviewer that never reached a desk is no failed hire.
            inner.hired.remove(terminal);
        }
        if matches!(self.terminals.wait_exit(terminal, Duration::ZERO), Ok(None))
            && let Ok(pid) = self.terminals.pid(terminal)
        {
            // SIGTERM once, never SIGKILL: one that outlives the wait is left to A Human.
            discovery::claude_code::terminate(pid);
            let _ = self.terminals.wait_exit(terminal, STOP_WAIT);
        }
        self.terminals.release(terminal);
        let mut inner = self.lock();
        inner.owned.remove(terminal);
        inner.ran_in.retain(|_, t| t != terminal);
        Ok(())
    }
}

/// Drop ended sessions that ran under `worktrees` from the sign-out sheet.
pub(super) fn hide_ended_reviewers(found: &mut Discovered, worktrees: &Path) {
    let hidden: Vec<String> = found
        .resume_dirs
        .iter()
        .filter(|(_, dir)| dir.starts_with(worktrees))
        .map(|(id, _)| id.clone())
        .collect();
    found.ended.retain(|e| !hidden.contains(&e.id));
    for id in &hidden {
        found.resume_dirs.remove(id);
        found.checkouts.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::floor_office;
    use super::*;
    use crate::discovery::claude_code::pid_alive;
    use crate::model::{EndedSession, TerminalExit};
    use std::path::PathBuf;
    use std::time::Instant;

    fn seat(office: &Office, project_id: &str, dir: &Path) -> Result<String, String> {
        office.seat_reviewer(
            project_id,
            &dir.join("home/code/app"),
            "/bin/cat",
            "QA review of #12",
        )
    }

    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn a_seated_reviewer_is_dismissed_and_its_terminal_released() {
        let (office, dir, floor) = floor_office("reviewer");
        let terminal = seat(&office, &floor.id, &dir).expect("seat");
        assert!(!office.reviewer_left(&terminal));
        assert_eq!(
            seat(&office, &floor.id, &dir),
            Err("a review is already running on this floor".to_string())
        );
        assert_eq!(
            seat(&office, "nope", &dir),
            Err("nope is not a floor of this building".to_string())
        );
        let pid = office.terminals.pid(&terminal).expect("pid");

        office.dismiss_reviewer(&terminal).expect("dismiss");
        wait_for("the reviewer to stop", || !pid_alive(pid));
        assert!(office.terminals.pid(&terminal).is_err());
        assert!(!office.lock().reviewers.contains_key(&terminal));
        assert!(!office.lock().owned.contains_key(&terminal));
        let exit = TerminalExit {
            terminal_id: terminal.clone(),
            exit_code: None,
        };
        assert_eq!(office.hire_failed(&exit), None);
        assert_eq!(
            office.dismiss_reviewer(&terminal),
            Err("that terminal is not a QA review".to_string())
        );
        // The floor is free again.
        let next = seat(&office, &floor.id, &dir).expect("seat again");
        office.dismiss_reviewer(&next).expect("dismiss again");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_plain_shell_cannot_be_dismissed_and_keeps_running() {
        let (office, dir, floor) = floor_office("reviewer-shell");
        let shell = office.open_shell(&floor.id).expect("open");
        let pid = office.terminals.pid(&shell.id).expect("pid");
        assert_eq!(
            office.dismiss_reviewer(&shell.id),
            Err("that terminal is not a QA review".to_string())
        );
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            office.terminals.wait_exit(&shell.id, Duration::ZERO),
            Ok(None)
        );
        assert!(pid_alive(pid));
        assert!(office.terminals.backlog(&shell.id).is_ok());
        // Not a reviewer, so it counts as one that left.
        assert!(office.reviewer_left(&shell.id));
        office.end_shell(&shell.id).expect("end");
        office
            .terminals
            .wait_exit(&shell.id, Duration::from_secs(10))
            .expect("wait");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_reviewer_that_exits_by_itself_has_left() {
        let (office, dir, floor) = floor_office("reviewer-exit");
        let terminal = seat(&office, &floor.id, &dir).expect("seat");
        // End-of-transmission on an empty line ends cat.
        office.terminals.write(&terminal, "\u{4}").expect("eot");
        wait_for("the reviewer to leave", || office.reviewer_left(&terminal));
        office.dismiss_reviewer(&terminal).expect("dismiss");
        assert!(office.terminals.pid(&terminal).is_err());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn ended_reviewers_leave_the_sign_out_sheet() {
        let worktrees = PathBuf::from("/data/qa/worktrees");
        let ended = |id: &str| EndedSession {
            id: id.to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: "p-1".to_string(),
            title: id.to_string(),
            ended_at: "2026-11-12T10:00:00Z".to_string(),
            model: None,
            effort: None,
        };
        let mut found = Discovered {
            ended: vec![ended("review"), ended("work")],
            ..Discovered::default()
        };
        for (id, dir) in [
            ("review", worktrees.join("p-1")),
            ("work", PathBuf::from("/code/app")),
        ] {
            found.resume_dirs.insert(id.to_string(), dir.clone());
            found.checkouts.insert(id.to_string(), dir);
        }
        // A sibling folder whose name only starts the same is not under it.
        found.ended.push(ended("near"));
        let near = PathBuf::from("/data/qa/worktrees-old/p-1");
        found.resume_dirs.insert("near".to_string(), near.clone());
        found.checkouts.insert("near".to_string(), near);

        hide_ended_reviewers(&mut found, &worktrees);
        let ids: Vec<&str> = found.ended.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["work", "near"]);
        assert!(!found.resume_dirs.contains_key("review"));
        assert!(!found.checkouts.contains_key("review"));
        assert_eq!(found.resume_dirs.len(), 2);
        assert_eq!(found.checkouts.len(), 2);
    }
}
