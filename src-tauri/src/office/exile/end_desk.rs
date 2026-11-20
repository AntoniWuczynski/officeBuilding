//! A floor's manager ends a desk the building started: one polite stop, the bounded wait, then
//! a forced end of the one process the building started in that desk's terminal and nothing
//! wider. The manager's files reach this through `end_floor_desk` alone.

use crate::delivery::gate::Route;
use crate::delivery::text;
use crate::discovery::claude_code::{is_unregistered_id, terminate};
use crate::manager::tools::EndDesk;
use crate::manager::wire::{EndHow, LogEvent};
use crate::office::manager_send::{log_event, may_act, refuse};
use crate::office::manager_tools::{NO_SUCH_DESK, floor_desk, json};
use crate::office::{Inner, Office, STOP_WAIT};
use serde::Serialize;
use std::time::Duration;

const TOOL: &str = "end_desk";
/// The most characters of the reason the manager gives.
const REASON_MAX: usize = 300;
/// How long the forced end has to show: the waiter thread records an exit within half a
/// second of reaping it.
const FORCE_WAIT: Duration = Duration::from_secs(3);

const PAUSED: &str = "the manager is paused, so no desk is ended";
const CHAT_VIEW: &str = "that desk runs in its tool's chat view and has no terminal of the building's, so only the human can end it";
const OUTSIDE: &str = "that desk was started outside the building, so the building does not end it";
const MOVING: &str = "that desk is being handed over, so it is not ended now";
const ALREADY: &str = "that desk is already being ended";
const NOT_ENDED: &str = "the desk did not end even when forced, so tell the human";

/// How a desk's process is ended. The app's own in `system`, recorders in tests.
pub(in crate::office) struct DeskEnder {
    /// Asks a process to exit. One SIGTERM in the app.
    pub(in crate::office) stop: Box<dyn Fn(i32) + Send + Sync>,
    /// Ends one that did not, and says whether it signalled. SIGKILL in the app, guarded by
    /// `own_running_child`.
    pub(in crate::office) force: Box<dyn Fn(i32) -> bool + Send + Sync>,
    /// `STOP_WAIT`, 5 seconds.
    pub(in crate::office) stop_wait: Duration,
    /// 3 seconds: the waiter thread records an exit within half a second of reaping it.
    pub(in crate::office) force_wait: Duration,
}

impl DeskEnder {
    /// The signals the app sends.
    pub(in crate::office) fn system() -> Self {
        Self {
            stop: Box::new(|pid| {
                if own_running_child(pid) {
                    terminate(pid);
                }
            }),
            force: Box::new(force_own_child),
            stop_wait: STOP_WAIT,
            force_wait: FORCE_WAIT,
        }
    }
}

#[derive(Serialize)]
struct Ended {
    /// How it went: "stopped" or "forced".
    outcome: EndHow,
    /// The session id that was ended.
    desk: String,
    title: String,
    detail: &'static str,
}

/// A desk that passed every check, held in its seat's `ending` set until the call is over.
struct Ending {
    terminal: String,
    session_id: String,
    title: String,
    reason: String,
}

/// Whether `pid` is still this app's own child, running and not yet reaped. The Probe (V9):
/// 0 with `si_pid` 0 for a running child, 0 with `si_pid` set for one that has exited, and
/// -1 for a pid that is not this process's un-reaped child. `info` is zeroed first because
/// macOS leaves it untouched when nothing has changed state.
#[expect(
    unsafe_code,
    reason = "waitid(2) has no safe std equivalent; WNOHANG with WNOWAIT only looks"
)]
fn own_running_child(pid: i32) -> bool {
    let Ok(id) = libc::id_t::try_from(pid) else {
        return false;
    };
    if id == 0 {
        return false;
    }
    // SAFETY: `info` is plain data the kernel fills in; the call changes no process's state.
    let (answered, exited) = unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        let rc = libc::waitid(
            libc::P_PID,
            id,
            &raw mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        );
        (rc == 0, info.si_pid)
    };
    answered && exited == 0
}

/// SIGKILL for one process, and only while it is this app's own running child. True when it
/// signalled. The check and the signal are two steps, so a child that exits and is reaped in
/// between could still be signalled: the window is microseconds and the waiter thread reaps
/// within DRAIN of the exit, which is why the check runs immediately before the signal.
#[expect(unsafe_code, reason = "kill(2) has no safe std equivalent")]
fn force_own_child(pid: i32) -> bool {
    if !own_running_child(pid) {
        return false;
    }
    // SAFETY: kill(2) touches no memory. A positive pid names one process, and the check
    // above says it was our own un-reaped child an instant ago.
    unsafe { libc::kill(pid, libc::SIGKILL) };
    true
}

impl Office {
    /// The one call the manager's files make. `DeskEnder::system()` and nothing else.
    pub(in crate::office) fn end_floor_desk(
        &self,
        floor: &str,
        args: &EndDesk,
    ) -> Result<String, String> {
        self.end_desk_through(floor, args, &DeskEnder::system())
    }

    /// The same with the signals given, for tests.
    pub(in crate::office) fn end_desk_through(
        &self,
        floor: &str,
        args: &EndDesk,
        ender: &DeskEnder,
    ) -> Result<String, String> {
        let ending = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            may_act(inner, floor, TOOL, PAUSED)?;
            let ending = self
                .in_reach(inner, floor, args)
                .map_err(|reason| refuse(inner, floor, TOOL, reason))?;
            if let Some(seat) = inner.seats.get_mut(floor) {
                seat.ending.insert(ending.terminal.clone());
            }
            ending
        };
        // No lock is held across the signals and the waits.
        let how = self.end_process(&ending.terminal, ender);
        let how = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            // The seat may be gone if the manager left meanwhile.
            if let Some(seat) = inner.seats.get_mut(floor) {
                seat.ending.remove(&ending.terminal);
            }
            let how = how.map_err(|reason| refuse(inner, floor, TOOL, reason))?;
            log_event(
                inner,
                floor,
                LogEvent::End {
                    session_id: ending.session_id.clone(),
                    title: ending.title.clone(),
                    reason: ending.reason,
                    how,
                },
            );
            // The desk is ended either way; the log is caught up at the next save.
            let _ = inner.store.save();
            how
        };
        self.refresh();
        json(&Ended {
            outcome: how,
            desk: ending.session_id,
            title: ending.title,
            detail: match how {
                EndHow::Stopped => {
                    "It stopped when asked. The human sees that you ended it and why, and can resume a Claude Code or Codex desk from the sign-out sheet."
                }
                EndHow::Forced => {
                    "It did not stop when asked, so the building ended its process. The human sees that you ended it and why, and can resume a Claude Code or Codex desk from the sign-out sheet."
                }
            },
        })
    }

    /// Whether a manager is ending the desk in `terminal` now.
    pub(in crate::office) fn being_ended(inner: &Inner, terminal: &str) -> bool {
        inner
            .seats
            .values()
            .any(|seat| seat.ending.contains(terminal))
    }

    /// The desk `args` names when this floor's manager may end it now, or why not, in the
    /// order of the checks.
    fn in_reach(&self, inner: &Inner, floor: &str, args: &EndDesk) -> Result<Ending, String> {
        let desk = floor_desk(self, inner, floor, &args.desk)?;
        let reason = text::line(&args.reason, REASON_MAX)
            .map_err(|e| format!("give the reason in one line: {e}"))?;
        let terminal = match (desk.desk.route, desk.terminal) {
            (Route::Structured, _) => return Err(CHAT_VIEW.to_string()),
            (_, None) => return Err(OUTSIDE.to_string()),
            (_, Some(terminal)) => terminal,
        };
        let by_hand =
            inner.shells.contains_key(&terminal) || inner.attached.values().any(|t| *t == terminal);
        if by_hand {
            return Err(OUTSIDE.to_string());
        }
        if is_unregistered_id(&args.desk) || is_unregistered_id(&desk.session_id) {
            return Err(NO_SUCH_DESK.to_string());
        }
        if inner.handover.taking_over.contains(&desk.session_id) {
            return Err(MOVING.to_string());
        }
        if Self::fixed_role(inner, &terminal) {
            return Err(NO_SUCH_DESK.to_string());
        }
        if inner.handover.underway.contains(&desk.session_id) {
            return Err(MOVING.to_string());
        }
        if Self::being_ended(inner, &terminal) {
            return Err(ALREADY.to_string());
        }
        let runs = self.terminals.wait_exit(&terminal, Duration::ZERO) == Ok(None);
        if !runs || inner.owned.get(&terminal) != Some(&desk.session_id) {
            return Err(NO_SUCH_DESK.to_string());
        }
        Ok(Ending {
            terminal,
            session_id: desk.session_id,
            title: desk.title,
            reason,
        })
    }

    /// One polite stop to the process the building started in `terminal`, the wait, then the
    /// forced end of that same process and no other.
    fn end_process(&self, terminal: &str, ender: &DeskEnder) -> Result<EndHow, String> {
        let pid = self.terminals.pid(terminal)?;
        // A released terminal answers Err: gone once the pid is no longer this app's child.
        let gone = |within| match self.terminals.wait_exit(terminal, within) {
            Ok(exit) => exit.is_some(),
            Err(_) => !own_running_child(pid),
        };
        (ender.stop)(pid);
        if gone(ender.stop_wait) {
            return Ok(EndHow::Stopped);
        }
        let forced = (ender.force)(pid);
        if gone(ender.force_wait) {
            return Ok(if forced {
                EndHow::Forced
            } else {
                EndHow::Stopped
            });
        }
        Err(NOT_ENDED.to_string())
    }
}

#[cfg(test)]
mod tests;
