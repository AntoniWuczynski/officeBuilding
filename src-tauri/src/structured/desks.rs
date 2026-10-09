//! The structured desks the building is running: each one's transcript, state
//! and open approval, kept for the conversation panel, and the driver that
//! talks to its agent. Driver events arrive on the drivers' own threads.

use super::{Approval, Driver, Entry, Event, Spawn};
use crate::model::{ControlMode, EndedSession, Session, SessionState, Spend, ToolKind};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::SystemTime;

/// What a structured desk shows, fixed when the agent was hired.
#[derive(Debug, Clone)]
pub struct DeskInfo {
    pub tool: ToolKind,
    pub project_id: String,
    pub root: PathBuf,
    pub title: String,
    pub model: String,
    pub effort: String,
}

struct Live {
    info: DeskInfo,
    transcript: Vec<Entry>,
    state: SessionState,
    approval: Option<Approval>,
    /// The tool's own id for the conversation, once the driver reports it.
    tool_session: Option<String>,
    last: SystemTime,
    ended: Option<String>,
}

/// Pushed to the conversation panel after every change to a desk.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub session_id: String,
    /// The entry that was added or replaced, if any.
    pub entry: Option<Entry>,
    pub state: SessionState,
    pub approval: Option<Approval>,
    /// Why the agent is gone, once it is.
    pub ended: Option<String>,
    /// The desk itself changed (state, or it left): the building should redraw.
    #[serde(skip)]
    pub desk_changed: bool,
}

/// Everything a panel needs when it opens on a desk.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Backlog {
    pub transcript: Vec<Entry>,
    pub state: SessionState,
    pub approval: Option<Approval>,
    pub ended: Option<String>,
}

pub type Notify = Arc<dyn Fn(Update) + Send + Sync>;

pub struct Desks {
    live: Mutex<BTreeMap<String, Live>>,
    drivers: Mutex<BTreeMap<String, Box<dyn Driver>>>,
    notify: Notify,
    seq: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock leaves plain data behind; keep serving it.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn iso(at: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(at).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Desks {
    #[must_use]
    pub fn new(notify: Notify) -> Arc<Self> {
        Arc::new(Self {
            live: Mutex::new(BTreeMap::new()),
            drivers: Mutex::new(BTreeMap::new()),
            notify,
            seq: AtomicU64::new(0),
        })
    }

    /// Start an agent on a new desk. Returns the desk's session id.
    pub fn start(self: &Arc<Self>, info: DeskInfo, spawn: &Spawn<'_>) -> Result<String, String> {
        let id = format!(
            "structured-{}",
            self.seq.fetch_add(1, Ordering::Relaxed) + 1
        );
        lock(&self.live).insert(
            id.clone(),
            Live {
                info,
                transcript: Vec::new(),
                state: SessionState::Thinking,
                approval: None,
                tool_session: None,
                last: SystemTime::now(),
                ended: None,
            },
        );
        match super::start(spawn, self.sink(&id)) {
            Ok(driver) => {
                lock(&self.drivers).insert(id.clone(), driver);
                Ok(id)
            }
            Err(e) => {
                lock(&self.live).remove(&id);
                Err(e)
            }
        }
    }

    /// Fold one driver event into its desk and tell the panel. Never takes the
    /// drivers lock: a driver may report while one of its methods runs.
    fn on_event(&self, id: &str, event: Event) {
        let update = {
            let mut live = lock(&self.live);
            let Some(desk) = live.get_mut(id) else {
                return;
            };
            let before = (desk.state, desk.ended.is_some());
            let mut entry = None;
            match event {
                Event::Entry(e) => {
                    match desk.transcript.iter_mut().find(|old| old.id() == e.id()) {
                        Some(old) => *old = e.clone(),
                        None => desk.transcript.push(e.clone()),
                    }
                    entry = Some(e);
                }
                Event::State(state) => desk.state = state,
                Event::SessionId(tool_session) => desk.tool_session = Some(tool_session),
                Event::Approval(approval) => {
                    desk.approval = Some(approval);
                    desk.state = SessionState::WaitingHuman;
                }
                Event::Resolved { approval_id } => {
                    if desk.approval.as_ref().is_some_and(|a| a.id == approval_id) {
                        desk.approval = None;
                    }
                }
                Event::Exited { code, message } => {
                    desk.approval = None;
                    desk.ended = Some(message.unwrap_or_else(|| match code {
                        Some(0) | None => "The agent has finished.".to_string(),
                        Some(code) => format!("The agent stopped with exit code {code}."),
                    }));
                }
            }
            desk.last = SystemTime::now();
            Update {
                session_id: id.to_string(),
                entry,
                state: desk.state,
                approval: desk.approval.clone(),
                ended: desk.ended.clone(),
                desk_changed: before != (desk.state, desk.ended.is_some()),
            }
        };
        (self.notify)(update);
    }

    /// The desks whose agents are still running, as sessions.
    #[must_use]
    pub fn sessions(&self) -> Vec<Session> {
        lock(&self.live)
            .iter()
            .filter(|(_, d)| d.ended.is_none())
            .map(|(id, d)| Session {
                id: id.clone(),
                tool: d.info.tool,
                project_id: d.info.project_id.clone(),
                title: d.info.title.clone(),
                state: d.state,
                control: ControlMode::Structured,
                model: Some(d.info.model.clone()),
                effort: Some(d.info.effort.clone()),
                last_activity_at: iso(d.last),
                pending_question_ids: Vec::new(),
                spend: Spend::default(),
                helpers: Vec::new(),
                worktree: false,
            })
            .collect()
    }

    /// Desks whose agents have ended and whose conversation can be reopened,
    /// newest first: they join their floor's sign-out sheet.
    #[must_use]
    pub fn ended(&self) -> Vec<EndedSession> {
        let live = lock(&self.live);
        let mut ended: Vec<(SystemTime, EndedSession)> = live
            .iter()
            .filter(|(_, d)| d.ended.is_some() && d.tool_session.is_some())
            .map(|(id, d)| {
                let session = EndedSession {
                    id: id.clone(),
                    tool: d.info.tool,
                    project_id: d.info.project_id.clone(),
                    title: d.info.title.clone(),
                    ended_at: iso(d.last),
                    model: Some(d.info.model.clone()),
                    effort: Some(d.info.effort.clone()),
                };
                (d.last, session)
            })
            .collect();
        ended.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        ended.into_iter().map(|(_, e)| e).collect()
    }

    /// The tools' own ids for the conversations of ended desks.
    #[must_use]
    pub fn ended_tool_sessions(&self) -> Vec<String> {
        lock(&self.live)
            .values()
            .filter(|d| d.ended.is_some())
            .filter_map(|d| d.tool_session.clone())
            .collect()
    }

    /// What an ended desk needs to reopen its conversation: how it was hired,
    /// and the tool's own id for the conversation.
    #[must_use]
    pub fn resumable(&self, id: &str) -> Option<(DeskInfo, String)> {
        let live = lock(&self.live);
        let desk = live.get(id).filter(|d| d.ended.is_some())?;
        Some((desk.info.clone(), desk.tool_session.clone()?))
    }

    /// Bring an ended desk back with a new driver reopening its conversation.
    /// Its transcript starts again from what the tool replays.
    pub fn resume(self: &Arc<Self>, id: &str, spawn: &Spawn<'_>) -> Result<(), String> {
        {
            let mut live = lock(&self.live);
            let desk = live
                .get_mut(id)
                .filter(|d| d.ended.is_some())
                .ok_or_else(|| format!("{id} has not ended, so there is nothing to resume"))?;
            desk.transcript.clear();
            desk.approval = None;
            desk.ended = None;
            desk.state = SessionState::Thinking;
            desk.last = SystemTime::now();
        }
        match super::start(spawn, self.sink(id)) {
            Ok(driver) => {
                lock(&self.drivers).insert(id.to_string(), driver);
                Ok(())
            }
            Err(e) => {
                if let Some(desk) = lock(&self.live).get_mut(id) {
                    desk.ended = Some(format!("It could not be resumed: {e}"));
                }
                Err(e)
            }
        }
    }

    fn sink(self: &Arc<Self>, id: &str) -> super::Sink {
        let weak: Weak<Self> = Arc::downgrade(self);
        let desk = id.to_string();
        Arc::new(move |event| {
            if let Some(desks) = weak.upgrade() {
                desks.on_event(&desk, event);
            }
        })
    }

    /// The folders running desks work in, so their floors stay up.
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        lock(&self.live)
            .values()
            .filter(|d| d.ended.is_none())
            .map(|d| d.info.root.clone())
            .collect()
    }

    /// The tools' own ids for running desks: discovery sees the same
    /// conversations in the tools' session files, and they must not get a second desk.
    #[must_use]
    pub fn tool_sessions(&self) -> Vec<String> {
        lock(&self.live)
            .values()
            .filter(|d| d.ended.is_none())
            .filter_map(|d| d.tool_session.clone())
            .collect()
    }

    pub fn backlog(&self, id: &str) -> Result<Backlog, String> {
        let live = lock(&self.live);
        let desk = live
            .get(id)
            .ok_or_else(|| format!("{id} is not a desk the app is running"))?;
        Ok(Backlog {
            transcript: desk.transcript.clone(),
            state: desk.state,
            approval: desk.approval.clone(),
            ended: desk.ended.clone(),
        })
    }

    fn with_driver<T>(
        &self,
        id: &str,
        run: impl FnOnce(&dyn Driver) -> Result<T, String>,
    ) -> Result<T, String> {
        let drivers = lock(&self.drivers);
        let driver = drivers
            .get(id)
            .ok_or_else(|| format!("{id} is not a desk the app is running"))?;
        run(driver.as_ref())
    }

    pub fn prompt(&self, id: &str, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("write something for the agent first".to_string());
        }
        self.with_driver(id, |d| d.prompt(text))
    }

    pub fn answer(
        &self,
        id: &str,
        approval_id: &str,
        choice_id: &str,
        values: &super::Values,
    ) -> Result<(), String> {
        self.with_driver(id, |d| d.answer(approval_id, choice_id, values))
    }

    pub fn cancel(&self, id: &str) -> Result<(), String> {
        self.with_driver(id, |d| d.cancel())
    }

    /// End the agent; its desk leaves once the driver reports the exit.
    pub fn stop(&self, id: &str) -> Result<(), String> {
        let driver = lock(&self.drivers)
            .remove(id)
            .ok_or_else(|| format!("{id} is not a desk the app is running"))?;
        driver.stop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structured::{Choice, ToolStatus};

    fn desks_with_one(updates: Arc<Mutex<Vec<Update>>>) -> Arc<Desks> {
        let desks = Desks::new(Arc::new(move |u| lock(&updates).push(u)));
        lock(&desks.live).insert(
            "structured-1".to_string(),
            Live {
                info: DeskInfo {
                    tool: ToolKind::Codex,
                    project_id: "p".to_string(),
                    root: PathBuf::from("/repo"),
                    title: "fix it".to_string(),
                    model: "default".to_string(),
                    effort: "default".to_string(),
                },
                transcript: Vec::new(),
                state: SessionState::Thinking,
                approval: None,
                tool_session: None,
                last: SystemTime::now(),
                ended: None,
            },
        );
        desks
    }

    #[test]
    fn streamed_text_replaces_its_entry_and_only_state_changes_redraw_the_desk() {
        let updates = Arc::new(Mutex::new(Vec::new()));
        let desks = desks_with_one(Arc::clone(&updates));
        let agent = |text: &str| {
            Event::Entry(Entry::Agent {
                id: "m1".to_string(),
                text: text.to_string(),
            })
        };
        desks.on_event("structured-1", agent("Hel"));
        desks.on_event("structured-1", agent("Hello"));
        desks.on_event("structured-1", Event::State(SessionState::Idle));
        let backlog = desks.backlog("structured-1").expect("backlog");
        assert_eq!(backlog.transcript.len(), 1);
        assert_eq!(
            backlog.transcript[0],
            Entry::Agent {
                id: "m1".to_string(),
                text: "Hello".to_string()
            }
        );
        let redraws: Vec<bool> = lock(&updates).iter().map(|u| u.desk_changed).collect();
        assert_eq!(redraws, vec![false, false, true]);
    }

    #[test]
    fn an_approval_raises_the_hand_until_it_is_resolved() {
        let updates = Arc::new(Mutex::new(Vec::new()));
        let desks = desks_with_one(updates);
        desks.on_event(
            "structured-1",
            Event::Approval(Approval {
                id: "a1".to_string(),
                title: "Run echo hi".to_string(),
                detail: String::new(),
                fields: Vec::new(),
                choices: vec![Choice {
                    id: "yes".to_string(),
                    label: "Allow".to_string(),
                    allow: true,
                }],
            }),
        );
        assert_eq!(desks.sessions()[0].state, SessionState::WaitingHuman);
        desks.on_event(
            "structured-1",
            Event::Resolved {
                approval_id: "a1".to_string(),
            },
        );
        desks.on_event(
            "structured-1",
            Event::Entry(Entry::Tool {
                id: "t1".to_string(),
                title: "echo hi".to_string(),
                status: ToolStatus::Done,
                detail: "hi".to_string(),
            }),
        );
        assert_eq!(
            desks.backlog("structured-1").expect("backlog").approval,
            None
        );
    }

    #[test]
    fn a_desk_leaves_when_its_agent_exits_and_its_tool_session_stays_hidden_till_then() {
        let updates = Arc::new(Mutex::new(Vec::new()));
        let desks = desks_with_one(updates);
        desks.on_event("structured-1", Event::SessionId("thread-9".to_string()));
        assert_eq!(desks.tool_sessions(), vec!["thread-9".to_string()]);
        assert_eq!(desks.roots(), vec![PathBuf::from("/repo")]);
        desks.on_event(
            "structured-1",
            Event::Exited {
                code: Some(2),
                message: None,
            },
        );
        assert!(desks.sessions().is_empty());
        assert!(desks.tool_sessions().is_empty());
        let ended = desks.backlog("structured-1").expect("backlog").ended;
        assert_eq!(
            ended.as_deref(),
            Some("The agent stopped with exit code 2.")
        );
        assert!(
            desks.prompt("structured-1", "hi").is_err(),
            "no driver for this test desk"
        );
    }

    /// A real opencode desk through the manager: hire, first answer, a second
    /// turn, then the agent leaves. Free model, no login needed.
    #[test]
    #[ignore = "runs opencode"]
    fn live_opencode_desk() {
        use crate::setup::Launch;
        use std::time::{Duration, Instant};
        let updates = Arc::new(Mutex::new(Vec::<Update>::new()));
        let seen = Arc::clone(&updates);
        let desks = Desks::new(Arc::new(move |u| lock(&seen).push(u)));
        let dir = std::env::temp_dir().join(format!("ob-desk-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let launch = Launch::default();
        let spawn = Spawn {
            tool: ToolKind::Opencode,
            cwd: &dir,
            model: Some("opencode/big-pickle"),
            effort: None,
            launch: &launch,
            task: "Reply with just the word ok.",
            resume: None,
        };
        let info = DeskInfo {
            tool: ToolKind::Opencode,
            project_id: "p".to_string(),
            root: dir.clone(),
            title: "live".to_string(),
            model: "opencode/big-pickle".to_string(),
            effort: "default".to_string(),
        };
        let id = desks.start(info, &spawn).expect("start");
        let wait_idle = |turns: usize| {
            let until = Instant::now() + Duration::from_secs(180);
            while Instant::now() < until {
                let b = desks.backlog(&id).expect("backlog");
                let agents = b
                    .transcript
                    .iter()
                    .filter(|e| matches!(e, Entry::Agent { .. }))
                    .count();
                if b.state == SessionState::Idle && agents >= turns {
                    return b;
                }
                std::thread::sleep(Duration::from_millis(300));
            }
            panic!("no answer in time: {:?}", desks.backlog(&id));
        };
        wait_idle(1);
        assert!(
            !desks.tool_sessions().is_empty(),
            "the agent's own session id arrived"
        );
        desks
            .prompt(&id, "Reply with just the word yes.")
            .expect("prompt");
        let b = wait_idle(2);
        assert!(matches!(b.transcript.first(), Some(Entry::User { .. })));
        desks.stop(&id).expect("stop");
        let until = Instant::now() + Duration::from_secs(20);
        while desks.backlog(&id).expect("backlog").ended.is_none() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(desks.sessions().is_empty(), "the desk left");
        assert!(lock(&updates).iter().any(|u| u.desk_changed));
        // Back from the sign-out sheet: the same desk, with the conversation replayed.
        let (info, tool_session) = desks.resumable(&id).expect("resumable");
        assert_eq!(desks.ended()[0].id, id);
        let again = Spawn {
            resume: Some(&tool_session),
            task: "",
            ..spawn
        };
        desks.resume(&id, &again).expect("resume");
        let until = Instant::now() + Duration::from_secs(120);
        loop {
            let b = desks.backlog(&id).expect("backlog");
            let users = b
                .transcript
                .iter()
                .filter(|e| matches!(e, Entry::User { .. }))
                .count();
            if b.state == SessionState::Idle && users >= 2 {
                break;
            }
            assert!(b.ended.is_none(), "resume failed: {b:?}");
            assert!(Instant::now() < until, "no replay in time: {b:?}");
            std::thread::sleep(Duration::from_millis(300));
        }
        assert_eq!(info.root, dir);
        assert_eq!(desks.sessions().len(), 1, "the desk is back");
        desks.stop(&id).expect("stop again");
    }
}
