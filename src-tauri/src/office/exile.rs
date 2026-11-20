//! Taking agents out of the building: sinking a floor with all its agents, or exiling one.

use super::launch::desk_session_id;
use super::{Inner, LAUNCH_POLL, Office, STOP_WAIT};
use crate::discovery;
use crate::model::Session;
use std::time::{Duration, Instant};

mod end_desk;

/// Ask each agent to exit (SIGTERM) and wait up to `within` for all of them.
pub(super) fn stop_agents(agents: &[(String, i32)], within: Duration) -> Result<(), String> {
    for (_, pid) in agents {
        discovery::claude_code::terminate(*pid);
    }
    let deadline = Instant::now() + within;
    loop {
        let left: Vec<&str> = agents
            .iter()
            .filter(|(_, pid)| discovery::claude_code::pid_alive(*pid))
            .map(|(title, _)| title.as_str())
            .collect();
        if left.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("{} did not stop", left.join(", ")));
        }
        std::thread::sleep(LAUNCH_POLL);
    }
}

impl Office {
    /// The process behind a session: discovery's, or the terminal we ran it in.
    fn agent_of(&self, inner: &Inner, s: &Session) -> Result<(String, i32), String> {
        let pid = inner.discovery.pid_of(&s.id).or_else(|| {
            inner
                .ran_in
                .get(&s.id)
                .and_then(|t| self.terminals.pid(t).ok())
        });
        let pid =
            pid.ok_or_else(|| format!("{} cannot be stopped: its process is unknown", s.title))?;
        Ok((s.title.clone(), pid))
    }

    /// Pull a floor down: stop its running agents cleanly, then hide it until a
    /// new session starts there. An agent that does not stop keeps it standing.
    pub fn demolish_floor(&self, project_id: &str) -> Result<(), String> {
        let (root, agents) = {
            let inner = self.lock();
            let root = inner
                .found
                .roots
                .get(project_id)
                .cloned()
                .ok_or_else(|| format!("{project_id} is not a floor of this building"))?;
            let mut agents = Vec::new();
            for s in inner
                .found
                .sessions
                .iter()
                .filter(|s| s.project_id == project_id)
            {
                if inner.handover.underway.contains(&s.id) {
                    return Err(format!(
                        "{} is being handed over, so the floor is still standing",
                        s.title
                    ));
                }
                agents.push(self.agent_of(&inner, s)?);
            }
            (root, agents)
        };
        stop_agents(&agents, STOP_WAIT)
            .map_err(|e| format!("{e}, so the floor is still standing"))?;
        self.hang_up_shells(project_id);
        let on_floor: Vec<String> = {
            let inner = self.lock();
            inner
                .found
                .sessions
                .iter()
                .filter(|s| s.project_id == project_id)
                .map(|s| s.id.clone())
                .collect()
        };
        self.hang_up_attached(&on_floor);
        {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let key = root.to_string_lossy().into_owned();
            let found = &inner.found;
            inner.desks.retain(|terminal, d| {
                let floor = found
                    .sessions
                    .iter()
                    .find(|s| s.id == desk_session_id(terminal))
                    .map_or(&d.project_id, |s| &s.project_id);
                floor != project_id
            });
            inner.store.records.pinned_floors.retain(|k| *k != key);
            let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            inner.store.records.demolished_floors.insert(key, at);
            inner.store.save()?;
        }
        self.refresh();
        Ok(())
    }

    /// Stop one session, as a floor's agents stop when it goes: its process, then its attach terminal.
    pub fn exile_session(&self, session_id: &str) -> Result<(), String> {
        {
            let inner = self.lock();
            if Self::staff_sessions(&inner).iter().any(|s| s == session_id) {
                return Err("a floor's manager or QA reviewer is not thrown out".to_string());
            }
            // The handover would start its receiver in place of the agent thrown out.
            if inner.handover.underway.contains(session_id) {
                return Err("this desk is being handed over".to_string());
            }
        }
        // An in-app desk is the app's own driver, not a process discovery found.
        if self
            .structured
            .sessions()
            .iter()
            .any(|s| s.id == session_id)
        {
            self.structured.stop(session_id)?;
            self.refresh();
            return Ok(());
        }
        let agent = {
            let inner = self.lock();
            let s = inner
                .found
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .ok_or_else(|| format!("unknown session: {session_id}"))?;
            self.agent_of(&inner, s)?
        };
        stop_agents(&[agent], STOP_WAIT)?;
        self.hang_up_attached(&[session_id.to_string()]);
        self.lock()
            .desks
            .retain(|terminal, _| desk_session_id(terminal) != session_id);
        self.refresh();
        Ok(())
    }
}
