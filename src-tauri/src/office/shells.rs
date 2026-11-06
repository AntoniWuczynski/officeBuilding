//! Plain shells the human opens on a floor (Cmd+N). Each runs in an app-owned
//! terminal like a hired agent's, but it is never a desk: the manager's office
//! lists it until it exits.

use super::{Office, now_iso};
use crate::model::{Shell, TerminalExit};
use crate::pty;
use portable_pty::CommandBuilder;

impl Office {
    /// Start a login shell in the floor's folder, as Terminal.app would.
    pub fn open_shell(&self, project_id: &str) -> Result<Shell, String> {
        let root = self
            .lock()
            .found
            .roots
            .get(project_id)
            .cloned()
            .ok_or_else(|| format!("{project_id} is not a floor of this building"))?;
        let mut cmd = CommandBuilder::new(pty::login_shell());
        cmd.arg("-l");
        let id = self.terminals.spawn(cmd, &root)?;
        let mut inner = self.lock();
        let number = inner
            .shells
            .values()
            .filter(|s| s.project_id == project_id)
            .map(|s| s.number)
            .max()
            .unwrap_or(0)
            + 1;
        let shell = Shell {
            id: id.clone(),
            project_id: project_id.to_string(),
            number,
            started_at: now_iso(),
        };
        inner.shells.insert(id, shell.clone());
        Ok(shell)
    }

    /// A terminal has exited: whether it was a shell, now gone from the list.
    pub fn shell_exited(&self, exit: &TerminalExit) -> bool {
        self.lock().shells.remove(&exit.terminal_id).is_some()
    }

    /// End one shell from the Shells panel. It leaves the list when its terminal exits.
    pub fn end_shell(&self, shell_id: &str) -> Result<(), String> {
        if !self.lock().shells.contains_key(shell_id) {
            return Err(format!("{shell_id} is not a shell"));
        }
        self.hang_up(shell_id);
        Ok(())
    }

    /// Hang up a demolished floor's shells, as closing their windows would.
    pub(super) fn hang_up_shells(&self, project_id: &str) {
        let inner = self.lock();
        for id in inner
            .shells
            .values()
            .filter(|s| s.project_id == project_id)
            .map(|s| &s.id)
        {
            self.hang_up(id);
        }
    }

    /// An interactive shell ignores SIGTERM, so it gets SIGHUP.
    #[expect(
        unsafe_code,
        reason = "kill(2) has no safe wrapper in the standard library"
    )]
    fn hang_up(&self, terminal_id: &str) {
        if let Ok(pid) = self.terminals.pid(terminal_id) {
            // SAFETY: kill(2) with SIGHUP touches no memory; a failure only means the shell is gone.
            unsafe { libc::kill(pid, libc::SIGHUP) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::floor_office;
    use crate::model::TerminalExit;
    use std::time::{Duration, Instant};

    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn a_shell_runs_in_the_floor_folder_and_leaves_the_list_when_it_exits() {
        let (office, dir, floor) = floor_office("shell");
        let first = office.open_shell(&floor.id).expect("open");
        let second = office.open_shell(&floor.id).expect("open again");
        assert_eq!((first.number, second.number), (1, 2));
        let snapshot = office.snapshot();
        assert_eq!(snapshot.shells.len(), 2);
        // A shell is not a desk.
        assert!(snapshot.sessions.is_empty());

        office.terminal_write(&first.id, "pwd\n").expect("type");
        wait_for("pwd", || {
            office
                .terminal_backlog(&first.id)
                .is_ok_and(|b| b.data.contains("code/app"))
        });

        office.terminal_write(&first.id, "exit\n").expect("exit");
        let code = office
            .terminals
            .wait_exit(&first.id, Duration::from_secs(10))
            .expect("wait")
            .expect("exited");
        let exit = TerminalExit {
            terminal_id: first.id.clone(),
            exit_code: code,
        };
        assert!(office.shell_exited(&exit));
        assert!(!office.shell_exited(&exit));
        let left: Vec<String> = office.snapshot().shells.into_iter().map(|s| s.id).collect();
        assert_eq!(left, vec![second.id.clone()]);
        // Not a hire, so its exit is no failed hire either.
        assert_eq!(office.hire_failed(&exit), None);

        office.terminal_write(&second.id, "exit\n").expect("exit");
        office
            .terminals
            .wait_exit(&second.id, Duration::from_secs(10))
            .expect("wait")
            .expect("exited");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn demolishing_a_floor_hangs_up_its_shells() {
        let (office, dir, floor) = floor_office("shell-demolish");
        let shell = office.open_shell(&floor.id).expect("open");
        office.demolish_floor(&floor.id).expect("demolish");
        assert!(
            office
                .terminals
                .wait_exit(&shell.id, Duration::from_secs(10))
                .expect("wait")
                .is_some()
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn ending_a_shell_hangs_it_up() {
        let (office, dir, floor) = floor_office("shell-end");
        let shell = office.open_shell(&floor.id).expect("open");
        office.end_shell(&shell.id).expect("end");
        assert!(
            office
                .terminals
                .wait_exit(&shell.id, Duration::from_secs(10))
                .expect("wait")
                .is_some()
        );
        assert!(
            office
                .end_shell("nope")
                .unwrap_err()
                .contains("not a shell")
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn an_unknown_floor_has_no_shell() {
        let (office, dir, _) = floor_office("shell-unknown");
        assert!(
            office
                .open_shell("nope")
                .unwrap_err()
                .contains("not a floor")
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
