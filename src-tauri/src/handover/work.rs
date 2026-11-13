//! What the hooks said about each session's background work, and what that rules out.

use super::{Task, TaskKind, Work, head, tidy_line};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

/// The most tasks one desk reports.
const TASKS_MAX: usize = 20;

/// How long a session discovery has not found yet keeps its signals.
const GRACE: Duration = Duration::from_secs(300);

/// What a Codex thread's rollout says about its goal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Goal {
    /// No goal was set, or its last recorded status is `complete` or `paused`.
    #[default]
    None,
    /// The last recorded status is any other word: it would carry on, or a limit stopped it.
    Standing { objective: String },
    /// The last word on the goal could not be read.
    Unknown,
}

/// The most characters of a goal's objective a task carries.
const OBJECTIVE_MAX: usize = 80;

/// A Codex desk's background work: its goal, and its sub-agent threads with a turn in flight.
#[must_use]
pub fn codex(goal: &Goal, busy_subagents: usize) -> Work {
    let objective = match goal {
        Goal::Unknown => return Work::Unknown,
        Goal::None => None,
        Goal::Standing { objective } => Some(objective),
    };
    let goal_task = objective.map(|text| {
        let line = tidy_line(text);
        let (kept, _) = head(&line, OBJECTIVE_MAX);
        Task {
            kind: TaskKind::Goal,
            name: Some(kept.to_owned()).filter(|n| !n.is_empty()),
        }
    });
    let subagents = std::iter::repeat_n(
        Task {
            kind: TaskKind::Subagent,
            name: None,
        },
        busy_subagents,
    );
    let tasks: Vec<Task> = goal_task
        .into_iter()
        .chain(subagents)
        .take(TASKS_MAX)
        .collect();
    if tasks.is_empty() {
        Work::Clear
    } else {
        Work::Running { tasks }
    }
}

/// One thing a hook said about a session's background work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// The work in flight at `at`. None: the hook did not list it.
    Listed {
        session_id: String,
        at: SystemTime,
        tasks: Option<Vec<Task>>,
    },
    /// A tool call that starts background work.
    Started {
        session_id: String,
        at: SystemTime,
        task: Task,
    },
}

#[derive(Debug, Default)]
struct Entry {
    listing: Option<(SystemTime, Option<Vec<Task>>)>,
    starts: Vec<(SystemTime, Task)>,
    /// What the desk's own file said outright.
    told: Option<Work>,
}

/// Per session: the newest listing, and the starts not older than it.
#[derive(Debug, Default)]
pub struct WorkLog {
    sessions: BTreeMap<String, Entry>,
}

impl WorkLog {
    pub fn note(&mut self, signal: Signal) {
        match signal {
            Signal::Listed {
                session_id,
                at,
                tasks,
            } => {
                let entry = self.sessions.entry(session_id).or_default();
                if entry.listing.as_ref().is_some_and(|(held, _)| *held > at) {
                    return;
                }
                entry.listing = Some((at, tasks));
                entry.starts.retain(|(started, _)| *started >= at);
            }
            Signal::Started {
                session_id,
                at,
                task,
            } => {
                let entry = self.sessions.entry(session_id).or_default();
                if entry.listing.as_ref().is_some_and(|(held, _)| *held > at) {
                    return;
                }
                entry.starts.push((at, task));
            }
        }
    }

    /// What a desk's own file says outright about its work. It replaces what was told
    /// before and is what `of` answers from then on.
    pub fn put(&mut self, session_id: &str, work: Work) {
        self.sessions.entry(session_id.to_owned()).or_default().told = Some(work);
    }

    /// What the desk's file told, else whether documented signals rule background work out.
    #[must_use]
    pub fn of(&self, session_id: &str) -> Work {
        if let Some(told) = self.sessions.get(session_id).and_then(|e| e.told.as_ref()) {
            return told.clone();
        }
        let Some(Entry {
            listing: Some((_, Some(listed))),
            starts,
            ..
        }) = self.sessions.get(session_id)
        else {
            return Work::Unknown;
        };
        let tasks: Vec<Task> = listed
            .iter()
            .cloned()
            .chain(starts.iter().map(|(_, task)| task.clone()))
            .take(TASKS_MAX)
            .collect();
        if tasks.is_empty() {
            Work::Clear
        } else {
            Work::Running { tasks }
        }
    }

    /// Forget sessions `live` is false for, unless a signal for them is newer than `GRACE`
    /// at `now`: a new desk is heard from before discovery finds its session.
    pub fn keep(&mut self, live: impl Fn(&str) -> bool, now: SystemTime) {
        self.sessions.retain(|id, entry| {
            let newest = entry
                .listing
                .iter()
                .map(|(at, _)| *at)
                .chain(entry.starts.iter().map(|(at, _)| *at))
                .max();
            live(id) || newest.is_some_and(|at| now.duration_since(at).is_ok_and(|age| age < GRACE))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handover::TaskKind;
    use std::time::Duration;

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn task(kind: TaskKind) -> Task {
        Task { kind, name: None }
    }

    fn listed(id: &str, at: u64, tasks: Option<Vec<Task>>) -> Signal {
        Signal::Listed {
            session_id: id.into(),
            at: t(at),
            tasks,
        }
    }

    fn started(id: &str, at: u64, kind: TaskKind) -> Signal {
        Signal::Started {
            session_id: id.into(),
            at: t(at),
            task: task(kind),
        }
    }

    fn running(kinds: &[TaskKind]) -> Work {
        Work::Running {
            tasks: kinds.iter().map(|k| task(*k)).collect(),
        }
    }

    #[test]
    fn the_work_table() {
        let cases: Vec<(&str, Vec<Signal>, Work)> = vec![
            ("nothing noted", vec![], Work::Unknown),
            (
                "empty listing",
                vec![listed("a", 1, Some(vec![]))],
                Work::Clear,
            ),
            ("unlisted", vec![listed("a", 1, None)], Work::Unknown),
            (
                "workflow listed",
                vec![listed("a", 1, Some(vec![task(TaskKind::Workflow)]))],
                running(&[TaskKind::Workflow]),
            ),
            (
                "empty then later start",
                vec![
                    listed("a", 1, Some(vec![])),
                    started("a", 2, TaskKind::Shell),
                ],
                running(&[TaskKind::Shell]),
            ),
            (
                "start then later empty listing",
                vec![
                    started("a", 1, TaskKind::Shell),
                    listed("a", 2, Some(vec![])),
                ],
                Work::Clear,
            ),
            (
                "start at the same instant",
                vec![
                    listed("a", 2, Some(vec![])),
                    started("a", 2, TaskKind::Subagent),
                ],
                running(&[TaskKind::Subagent]),
            ),
            (
                "start first, same instant listing",
                vec![
                    started("a", 2, TaskKind::Subagent),
                    listed("a", 2, Some(vec![])),
                ],
                running(&[TaskKind::Subagent]),
            ),
            (
                "older listing after a newer one",
                vec![
                    listed("a", 5, Some(vec![])),
                    listed("a", 3, Some(vec![task(TaskKind::Workflow)])),
                ],
                Work::Clear,
            ),
            (
                "older start after a newer listing",
                vec![
                    listed("a", 5, Some(vec![])),
                    started("a", 3, TaskKind::Shell),
                ],
                Work::Clear,
            ),
            (
                "a start alone",
                vec![started("a", 1, TaskKind::Shell)],
                Work::Unknown,
            ),
            (
                "two sessions do not mix",
                vec![
                    listed("a", 1, Some(vec![task(TaskKind::Workflow)])),
                    listed("b", 1, Some(vec![])),
                ],
                running(&[TaskKind::Workflow]),
            ),
        ];
        for (name, signals, want) in cases {
            let mut log = WorkLog::default();
            for s in signals {
                log.note(s);
            }
            assert_eq!(log.of("a"), want, "{name}");
        }
    }

    fn standing(objective: &str) -> Goal {
        Goal::Standing {
            objective: objective.into(),
        }
    }

    fn named(kind: TaskKind, name: Option<&str>) -> Task {
        Task {
            kind,
            name: name.map(str::to_owned),
        }
    }

    #[test]
    fn the_codex_work_table() {
        let sub = || named(TaskKind::Subagent, None);
        let long = "x".repeat(200);
        let cases: Vec<(&str, Goal, usize, Work)> = vec![
            ("unknown with nothing", Goal::Unknown, 0, Work::Unknown),
            ("unknown with busy", Goal::Unknown, 3, Work::Unknown),
            ("nothing", Goal::None, 0, Work::Clear),
            (
                "standing goal",
                standing("Finish the migration"),
                0,
                Work::Running {
                    tasks: vec![named(TaskKind::Goal, Some("Finish the migration"))],
                },
            ),
            (
                "long objective",
                standing(&long),
                0,
                Work::Running {
                    tasks: vec![named(TaskKind::Goal, Some(&"x".repeat(80)))],
                },
            ),
            (
                "line break and escape",
                standing("one\ntwo\x1b[31m red"),
                0,
                Work::Running {
                    tasks: vec![named(TaskKind::Goal, Some("one two red"))],
                },
            ),
            (
                "empty objective",
                standing(""),
                0,
                Work::Running {
                    tasks: vec![named(TaskKind::Goal, None)],
                },
            ),
            (
                "goal then two sub-agents",
                standing("g"),
                2,
                Work::Running {
                    tasks: vec![named(TaskKind::Goal, Some("g")), sub(), sub()],
                },
            ),
            (
                "sub-agents alone",
                Goal::None,
                1,
                Work::Running { tasks: vec![sub()] },
            ),
        ];
        for (name, goal, busy, want) in cases {
            assert_eq!(codex(&goal, busy), want, "{name}");
        }
        match codex(&standing("g"), 30) {
            Work::Running { tasks } => {
                assert_eq!(tasks.len(), 20);
                assert_eq!(tasks[0].kind, TaskKind::Goal);
            }
            other => panic!("expected running, got {other:?}"),
        }
    }

    #[test]
    fn put_is_what_of_answers() {
        let mut log = WorkLog::default();
        log.put("a", Work::Clear);
        assert_eq!(log.of("a"), Work::Clear);
        let running = codex(&standing("g"), 0);
        log.put("a", running.clone());
        assert_eq!(log.of("a"), running);
        assert_eq!(log.of("b"), Work::Unknown);
    }

    #[test]
    fn hook_signals_read_as_before_until_something_is_told() {
        let mut log = WorkLog::default();
        log.note(listed("a", 1, Some(vec![])));
        assert_eq!(log.of("a"), Work::Clear);
        log.put("a", Work::Unknown);
        assert_eq!(log.of("a"), Work::Unknown);
        let mut log = WorkLog::default();
        log.put("a", Work::Clear);
        log.note(listed("a", 1, Some(vec![task(TaskKind::Shell)])));
        assert_eq!(log.of("a"), Work::Clear);
    }

    #[test]
    fn keep_forgets_what_was_told_with_the_session() {
        let mut log = WorkLog::default();
        log.put("a", Work::Clear);
        log.put("b", Work::Clear);
        log.keep(|id| id == "a", t(1_000_000));
        assert_eq!(log.of("a"), Work::Clear);
        assert_eq!(log.of("b"), Work::Unknown);
    }

    #[test]
    fn the_other_session_is_separate() {
        let mut log = WorkLog::default();
        log.note(listed("a", 1, Some(vec![task(TaskKind::Workflow)])));
        log.note(listed("b", 1, Some(vec![])));
        assert_eq!(log.of("b"), Work::Clear);
        assert_eq!(log.of("c"), Work::Unknown);
    }

    #[test]
    fn keep_forgets_sessions_that_are_gone() {
        let mut log = WorkLog::default();
        log.note(listed("a", 1, Some(vec![])));
        log.note(listed("b", 1, Some(vec![])));
        log.keep(|id| id == "a", t(1_000_000));
        assert_eq!(log.of("a"), Work::Clear);
        assert_eq!(log.of("b"), Work::Unknown);
    }

    #[test]
    fn keep_holds_a_session_discovery_has_not_found_yet_for_a_few_minutes() {
        let mut log = WorkLog::default();
        log.note(listed("new", 1_000, Some(vec![])));
        log.keep(|_| false, t(1_100));
        assert_eq!(log.of("new"), Work::Clear);
        log.keep(|_| false, t(1_400));
        assert_eq!(log.of("new"), Work::Unknown);
    }

    #[test]
    fn more_than_twenty_tasks_are_cut_to_twenty() {
        let mut log = WorkLog::default();
        log.note(listed("a", 1, Some(vec![task(TaskKind::Shell); 15])));
        for i in 0..10 {
            log.note(started("a", 2 + i, TaskKind::Subagent));
        }
        match log.of("a") {
            Work::Running { tasks } => assert_eq!(tasks.len(), 20),
            other => panic!("expected running, got {other:?}"),
        }
    }
}
