//! `hire_desk` and `wait_for_desk`. A hire is checked against the floor's switch and the caps in
//! the backend, one hire at a time, and starts an ordinary desk in the floor's own folder.

use super::launch::{chosen_model, hireable};
use super::manager::Hire;
use super::manager_send::{log_event, may_act, refuse};
use super::manager_tools::{floor_desk, json};
use super::{Inner, Office};
use crate::delivery::text;
use crate::manager::tools::{HireDesk, HireTool, WaitForDesk};
use crate::manager::wire::LogEvent;
use crate::model::{HireRequest, SessionState, ToolKind};
use crate::settings::checked_names;
use serde::Serialize;
use std::time::{Duration, Instant};

const TOOL: &str = "hire_desk";
/// The most characters a hired desk's task may hold.
const TASK_MAX: usize = 2000;
/// How long `wait_for_desk` waits by default and at most, and how often it looks.
const WAIT_SECONDS: u32 = 30;
const WAIT_SECONDS_MAX: u32 = 50;
const WAIT_LOOK: Duration = Duration::from_millis(500);

#[derive(Serialize)]
struct Hired {
    outcome: &'static str,
    task: String,
    note: &'static str,
}

#[derive(Serialize)]
struct Waited {
    state: SessionState,
    changed: bool,
    left: bool,
}

/// How long a wait lasts, in seconds: 30 unless asked, and 1 to 50.
fn wait_seconds(asked: Option<u32>) -> u32 {
    asked.unwrap_or(WAIT_SECONDS).clamp(1, WAIT_SECONDS_MAX)
}

/// A hire that passed its checks: the cleaned task and the names to start it with.
pub(super) struct Checked {
    pub(super) task: String,
    pub(super) model: Option<String>,
    pub(super) effort: Option<String>,
}

impl Office {
    /// Whether the floor's manager may hire now, under the office lock. Every refusal is logged.
    pub(super) fn hire_check(
        &self,
        inner: &mut Inner,
        floor: &str,
        args: &HireDesk,
    ) -> Result<Checked, String> {
        may_act(
            inner,
            floor,
            TOOL,
            "the manager is paused, so nobody is hired",
        )?;
        let refused = |inner: &mut Inner, reason: String| Err(refuse(inner, floor, TOOL, reason));
        if !Self::manager_record(inner, floor).may_hire {
            let reason = "hiring is switched off for this floor. The human can switch it on in the Manager panel";
            return refused(inner, reason.to_string());
        }
        let caps = self.manager_caps(inner, floor);
        if caps.hired_now >= caps.max_desks {
            let reason = format!(
                "you already have {} desks hired, which is the cap",
                caps.hired_now
            );
            return refused(inner, reason);
        }
        if caps.spent_usd >= f64::from(caps.spend_ceiling_usd) {
            let reason = format!(
                "the desks you hired have spent ${:.2}, which reaches the ceiling of ${}",
                caps.spent_usd, caps.spend_ceiling_usd
            );
            return refused(inner, reason);
        }
        let checked = text::line(&args.task, TASK_MAX).and_then(|task| {
            let (model, effort) = checked_names(args.model.clone(), args.effort.clone())?;
            Ok(Checked {
                task,
                model,
                effort,
            })
        });
        checked.or_else(|reason| refused(inner, reason))
    }

    /// Record a desk the floor's manager hired, in its seat and its log.
    pub(super) fn record_hire(
        inner: &mut Inner,
        floor: &str,
        terminal: String,
        tool: ToolKind,
        task: String,
    ) {
        if let Some(seat) = inner.seats.get_mut(floor) {
            seat.hires.push(Hire {
                terminal,
                usd: 0.0,
                unpriced: false,
            });
        }
        log_event(inner, floor, LogEvent::Hire { tool, task });
        // The desk is running either way; the log is caught up at the next save.
        let _ = inner.store.save();
    }

    /// Hire one more desk on the manager's floor, within the switch and the caps.
    pub(super) fn hire_desk(&self, floor: &str, args: &HireDesk) -> Result<String, String> {
        // One hire at a time: the checks still hold when the hire is recorded.
        let _one = crate::sync::lock(&self.hiring);
        let checked = self.hire_check(&mut self.lock(), floor, args)?;
        let tool = match args.tool {
            HireTool::ClaudeCode => ToolKind::ClaudeCode,
            HireTool::Codex => ToolKind::Codex,
        };
        let started = hireable(self.hire_options(), tool)
            .and_then(|options| {
                let (model, effort) = chosen_model(
                    &options,
                    checked.model.as_deref(),
                    checked.effort.as_deref(),
                )?;
                Ok(HireRequest {
                    tool,
                    title: checked.task.clone(),
                    model: model.id.clone(),
                    effort,
                    ultracode: false,
                    structured: false,
                })
            })
            .and_then(|req| self.spawn_in_terminal(floor, &req))
            .and_then(|(_, terminal)| {
                terminal.ok_or_else(|| "the desk has no terminal".to_string())
            });
        let mut inner = self.lock();
        let terminal = started.map_err(|reason| refuse(&mut inner, floor, TOOL, reason))?;
        Self::record_hire(&mut inner, floor, terminal, tool, checked.task.clone());
        json(&Hired {
            outcome: "hired",
            task: checked.task,
            note: "It appears in list_desks once it has started work.",
        })
    }

    /// Follow the spend of every desk a manager hired upward: the highest figure seen stays,
    /// also once the desk has left. True when a figure changed.
    pub(super) fn follow_hire_spend(inner: &mut Inner) -> bool {
        let Inner {
            seats,
            owned,
            found,
            ..
        } = inner;
        let mut changed = false;
        for hire in seats.values_mut().flat_map(|seat| seat.hires.iter_mut()) {
            let Some(session) = owned
                .get(&hire.terminal)
                .and_then(|id| found.sessions.iter().find(|s| s.id == *id))
            else {
                continue;
            };
            let usd = hire.usd.max(session.spend.usd);
            let unpriced = session.spend.unpriced_tokens > 0;
            changed |= usd > hire.usd || unpriced != hire.unpriced;
            hire.usd = usd;
            hire.unpriced = unpriced;
        }
        changed
    }

    /// Wait until a desk's state changes or it leaves, without holding the lock between looks.
    pub(super) fn wait_for_desk(&self, floor: &str, args: &WaitForDesk) -> Result<String, String> {
        let until = Instant::now() + Duration::from_secs(u64::from(wait_seconds(args.seconds)));
        let first = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            match floor_desk(self, inner, floor, &args.desk) {
                Ok(desk) => desk.desk.state,
                Err(reason) => return Err(refuse(inner, floor, "wait_for_desk", reason)),
            }
        };
        loop {
            let now = {
                let inner = self.lock();
                floor_desk(self, &inner, floor, &args.desk).map(|desk| desk.desk.state)
            };
            let waited = match now {
                Err(_) => Some(Waited {
                    state: first,
                    changed: true,
                    left: true,
                }),
                Ok(state) if state != first => Some(Waited {
                    state,
                    changed: true,
                    left: false,
                }),
                Ok(state) => (Instant::now() >= until).then_some(Waited {
                    state,
                    changed: false,
                    left: false,
                }),
            };
            if let Some(waited) = waited {
                return json(&waited);
            }
            std::thread::sleep(WAIT_LOOK.min(until.saturating_duration_since(Instant::now())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::manager::tests::{bind, manager_office, seat, session, wait_for};
    use super::super::manager_send::NO_SEAT;
    use super::super::tests::floor_office;
    use super::*;
    use crate::manager::tools::{Call, Tools};
    use crate::model::ControlMode;
    use crate::settings::QA_NAME_MAX;
    use portable_pty::CommandBuilder;
    use serde::Deserialize;
    use std::fs;
    use std::io::Write;

    fn args(task: &str) -> HireDesk {
        HireDesk {
            tool: HireTool::ClaudeCode,
            task: task.to_string(),
            model: None,
            effort: None,
        }
    }

    fn check(office: &Office, floor: &str, args: &HireDesk) -> Result<Checked, String> {
        office.hire_check(&mut office.lock(), floor, args)
    }

    fn refusals(office: &Office, floor: &str) -> Vec<String> {
        Office::manager_record(&office.lock(), floor)
            .log
            .into_iter()
            .filter_map(|entry| match entry.event {
                LogEvent::Refused { tool, reason } if tool == TOOL => Some(reason),
                _ => None,
            })
            .collect()
    }

    fn cat(office: &Office, dir: &std::path::Path) -> String {
        office
            .terminals
            .spawn(CommandBuilder::new("/bin/cat"), &dir.join("home/code/app"))
            .unwrap()
    }

    #[test]
    fn the_hire_check_refuses_with_its_words_and_passes_otherwise() {
        let (office, dir, floor, registry, _) = manager_office("hire-check");
        assert_eq!(
            check(&office, &floor.id, &args("go")).err(),
            Some(NO_SEAT.to_string())
        );
        seat(&office, &dir, &floor, &registry, "/bin/cat");
        let off = "hiring is switched off for this floor. The human can switch it on in the Manager panel";
        assert_eq!(
            check(&office, &floor.id, &args("go")).err(),
            Some(off.to_string())
        );

        office.set_manager_hiring(&floor.id, true).unwrap();
        let named = HireDesk {
            model: Some("  opus ".to_string()),
            effort: Some(String::new()),
            ..args("Fix\n  the tests")
        };
        let passed = check(&office, &floor.id, &named).unwrap();
        assert_eq!(passed.task, "Fix the tests");
        assert_eq!(
            (passed.model.as_deref(), passed.effort),
            (Some("opus"), None)
        );
        let long_model = HireDesk {
            model: Some("x".repeat(QA_NAME_MAX + 1)),
            ..args("go")
        };
        let bad_model = "that model name is not valid".to_string();
        assert_eq!(
            check(&office, &floor.id, &long_model).err(),
            Some(bad_model.clone())
        );
        let odd_effort = HireDesk {
            effort: Some("hi\u{7}gh".to_string()),
            ..args("go")
        };
        let bad_effort = "that effort name is not valid".to_string();
        assert_eq!(
            check(&office, &floor.id, &odd_effort).err(),
            Some(bad_effort.clone())
        );
        let empty = "there is nothing to send".to_string();
        assert_eq!(
            check(&office, &floor.id, &args(" \n ")).err(),
            Some(empty.clone())
        );

        for n in 0..3 {
            let terminal = cat(&office, &dir);
            Office::record_hire(
                &mut office.lock(),
                &floor.id,
                terminal,
                ToolKind::Codex,
                format!("task {n}"),
            );
        }
        let cap = "you already have 3 desks hired, which is the cap".to_string();
        assert_eq!(
            check(&office, &floor.id, &args("go")).err(),
            Some(cap.clone())
        );

        office.lock().seats.get_mut(&floor.id).unwrap().hires = vec![Hire {
            terminal: "t-gone".to_string(),
            usd: 20.0,
            unpriced: false,
        }];
        let ceiling =
            "the desks you hired have spent $20.00, which reaches the ceiling of $20".to_string();
        assert_eq!(
            check(&office, &floor.id, &args("go")).err(),
            Some(ceiling.clone())
        );

        office.set_manager_paused(&floor.id, true).unwrap();
        let on_pause = "the manager is paused, so nobody is hired".to_string();
        assert_eq!(
            check(&office, &floor.id, &args("go")).err(),
            Some(on_pause.clone())
        );
        assert_eq!(
            refusals(&office, &floor.id),
            vec![
                off.to_string(),
                bad_model,
                bad_effort,
                empty,
                cap,
                ceiling,
                on_pause
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_recorded_hire_counts_toward_the_cap_until_its_terminal_exits() {
        let (office, dir, floor, registry, _) = manager_office("hire-count");
        seat(&office, &dir, &floor, &registry, "/bin/cat");
        // `cat` reads a pipe: closing the pipe's only writer ends it.
        let pipe = dir.join("pipe");
        let made = std::process::Command::new("/usr/bin/mkfifo")
            .arg(&pipe)
            .status()
            .unwrap();
        assert!(made.success());
        let mut command = CommandBuilder::new("/bin/cat");
        command.arg(&pipe);
        let terminal = office
            .terminals
            .spawn(command, &dir.join("home/code/app"))
            .unwrap();
        Office::record_hire(
            &mut office.lock(),
            &floor.id,
            terminal.clone(),
            ToolKind::Codex,
            "Write the docs".to_string(),
        );
        let hired_now = || office.manager_caps(&office.lock(), &floor.id).hired_now;
        assert_eq!(hired_now(), 1);
        // macOS leaves `cat` asleep in its open() when the writer has gone before it wakes, so
        // the pipe is held until `cat` has read from it.
        let mut writer = fs::OpenOptions::new().write(true).open(&pipe).unwrap();
        writer.write_all(b"pipe-open\n").unwrap();
        wait_for("cat to read the pipe", || {
            let shown = office.terminals.backlog(&terminal).unwrap().data;
            shown.contains("pipe-open")
        });
        drop(writer);
        wait_for("the hired desk to exit", || hired_now() == 0);
        let logged = Office::manager_record(&office.lock(), &floor.id).log;
        assert!(matches!(
            &logged[..],
            [entry] if entry.event == LogEvent::Hire { tool: ToolKind::Codex, task: "Write the docs".to_string() }
        ));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_spend_of_a_hire_only_goes_up_and_outlasts_its_desk() {
        let (office, dir, floor, registry, _) = manager_office("hire-spend");
        seat(&office, &dir, &floor, &registry, "/bin/cat");
        let terminal = cat(&office, &dir);
        Office::record_hire(
            &mut office.lock(),
            &floor.id,
            terminal.clone(),
            ToolKind::ClaudeCode,
            "go".to_string(),
        );
        bind(&office, &terminal, "s-hired", &floor.id);
        let set = |usd: f64, unpriced: u64| {
            let mut inner = office.lock();
            let s = inner
                .found
                .sessions
                .iter_mut()
                .find(|s| s.id == "s-hired")
                .unwrap();
            s.spend.usd = usd;
            s.spend.unpriced_tokens = unpriced;
        };
        let caps = || office.manager_caps(&office.lock(), &floor.id);

        set(2.0, 0);
        assert!(Office::follow_hire_spend(&mut office.lock()));
        assert!((caps().spent_usd - 2.0).abs() < f64::EPSILON);
        set(1.0, 0);
        assert!(!Office::follow_hire_spend(&mut office.lock()), "never down");
        assert!((caps().spent_usd - 2.0).abs() < f64::EPSILON);
        set(3.0, 10);
        assert!(office.deliver_due(), "every pass follows the spend");
        assert!((caps().spent_usd - 3.0).abs() < f64::EPSILON);
        assert_eq!(caps().unpriced_desks, 1);

        office.lock().found.sessions.retain(|s| s.id != "s-hired");
        assert!(!Office::follow_hire_spend(&mut office.lock()));
        assert!(
            (caps().spent_usd - 3.0).abs() < f64::EPSILON,
            "a desk that left keeps its figure"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestWaited {
        state: SessionState,
        changed: bool,
        left: bool,
    }

    fn wait(office: &Office, floor: &str, desk: &str, seconds: u32) -> Result<TestWaited, String> {
        let call = Call::WaitForDesk(WaitForDesk {
            desk: desk.to_string(),
            seconds: Some(seconds),
        });
        office
            .call(floor, call)
            .map(|answer| serde_json::from_str(&answer).unwrap())
    }

    #[test]
    fn a_wait_ends_on_a_change_a_departure_or_its_limit() {
        let (office, dir, floor) = floor_office("hire-wait");
        office
            .lock()
            .found
            .sessions
            .push(session("s-hand", &floor.id, ControlMode::RaiseWindow));
        let later = |change: fn(&mut Vec<crate::model::Session>)| {
            std::thread::sleep(Duration::from_millis(300));
            change(&mut office.lock().found.sessions);
        };

        let flipped = std::thread::scope(|scope| {
            scope.spawn(|| {
                later(|sessions| sessions[0].state = SessionState::Running);
            });
            wait(&office, &floor.id, "s-hand", 10).unwrap()
        });
        assert_eq!(
            (flipped.state, flipped.changed, flipped.left),
            (SessionState::Running, true, false)
        );

        let started = Instant::now();
        let still = wait(&office, &floor.id, "s-hand", 1).unwrap();
        assert_eq!(
            (still.state, still.changed, still.left),
            (SessionState::Running, false, false)
        );
        assert!(
            started.elapsed() >= Duration::from_secs(1)
                && started.elapsed() < Duration::from_secs(3)
        );

        let gone = std::thread::scope(|scope| {
            scope.spawn(|| later(Vec::clear));
            wait(&office, &floor.id, "s-hand", 10).unwrap()
        });
        assert_eq!(
            (gone.state, gone.changed, gone.left),
            (SessionState::Running, true, true)
        );
        assert_eq!(
            wait(&office, &floor.id, "s-hand", 1).err(),
            Some("there is no such desk on your floor".to_string())
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_wait_lasts_one_to_fifty_seconds() {
        assert_eq!(wait_seconds(None), 30);
        assert_eq!(wait_seconds(Some(0)), 1);
        assert_eq!(wait_seconds(Some(7)), 7);
        assert_eq!(wait_seconds(Some(1000)), 50);
    }
}
