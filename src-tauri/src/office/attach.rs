//! A Claude Code background session opened inside the app: a docked terminal running
//! `claude attach <job>`.

use super::{Office, start_agent_with};
use crate::{focus, pty};
use std::path::Path;
use std::time::Duration;

pub(super) fn attach_command(job: &str, shell: &str) -> String {
    format!("claude attach {}", pty::quote(shell, job))
}

impl Office {
    /// Open a background session in a docked terminal running `claude attach <job>`.
    /// Ok(false) when it is not a background session, so its own window is raised instead.
    pub fn attach_session(&self, session_id: &str) -> Result<bool, String> {
        let (dir, pid) = {
            let inner = self.lock();
            let session = inner
                .found
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .ok_or_else(|| format!("unknown session: {session_id}"))?;
            let dir = inner
                .found
                .roots
                .get(&session.project_id)
                .unwrap_or(&self.home);
            (dir.clone(), inner.discovery.pid_of(session_id))
        };
        let Some(job) = pid.and_then(|p| focus::background_job_of(&self.home, p)) else {
            return Ok(false);
        };
        let command = attach_command(&job, &pty::login_shell());
        self.attach_in_terminal(session_id, &dir, &command)?;
        Ok(true)
    }

    /// Run `command` in a docked terminal for `session_id`, unless one is still running.
    /// Not a hire: its exit is not a failure to report, and it is no desk of its own.
    pub(super) fn attach_in_terminal(
        &self,
        session_id: &str,
        dir: &Path,
        command: &str,
    ) -> Result<(), String> {
        let live = |t: &String| self.terminals.wait_exit(t, Duration::ZERO) == Ok(None);
        if self.lock().attached.get(session_id).is_some_and(live) {
            return Ok(());
        }
        let terminal = start_agent_with(&self.terminals, dir, command, &[])?;
        let mut inner = self.lock();
        // A second click may have attached while this one was starting: keep the first.
        if inner.attached.get(session_id).is_some_and(live) {
            drop(inner);
            self.hang_up(&terminal);
            self.terminals.release(&terminal);
            return Ok(());
        }
        if let Some(old) = inner.attached.insert(session_id.to_string(), terminal) {
            self.terminals.release(&old);
        }
        Ok(())
    }

    /// Let go of the app's `claude attach` for one session, so a terminal of the
    /// user's own keeps the session at its full size. A no-op when none is attached.
    pub fn detach_session(&self, session_id: &str) {
        self.hang_up_attached(&[session_id.to_string()]);
    }

    /// Hang up and forget the attach terminals of the sessions `ids`, as they go.
    pub(super) fn hang_up_attached(&self, ids: &[String]) {
        let gone: Vec<String> = {
            let mut inner = self.lock();
            ids.iter()
                .filter_map(|id| inner.attached.remove(id))
                .collect()
        };
        for terminal in gone {
            self.hang_up(&terminal);
            self.terminals.release(&terminal);
        }
    }
}
