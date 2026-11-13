//! What the hooks said about each session's background work, and what that rules out.

use super::{Task, Work};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

/// The most tasks one desk reports.
const TASKS_MAX: usize = 20;

/// How long a session discovery has not found yet keeps its signals.
const GRACE: Duration = Duration::from_secs(300);

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

    /// Whether documented signals rule background work out for this session.
    #[must_use]
    pub fn of(&self, session_id: &str) -> Work {
        let Some(Entry {
            listing: Some((_, Some(listed))),
            starts,
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
