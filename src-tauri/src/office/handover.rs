//! Handing a desk over between the Claude and Codex subscriptions: which desks are the
//! building's own, one pass of the watch loop over them, and A Human's manual handover.
//! Carrying one out is in `handover/run.rs`. Nothing here ends or starts an agent.

use super::{Inner, Office, STOP_WAIT};
use crate::handover::record::{Book, Done, HandoverFile, Outcome, RESET_HORIZON, UsedUp};
use crate::handover::survey::{DeskFacts, Now, Survey, survey};
use crate::handover::view::{BlockWhy, DeskState, HandoverView};
use crate::handover::work::{self, WorkLog};
use crate::handover::{Allowance, Tool, Work};
use crate::model::ControlMode;
use crate::process::{Runner, SystemRunner};
use crate::settings::{HandoverSettings, StartWith};
use crate::sync::lock;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
mod codex_tests;
mod desk;
#[cfg(test)]
mod fixture;
mod run;
#[cfg(test)]
mod tests;

/// What the office needs to hand desks over.
pub struct HandoverHost {
    pub runner: Arc<dyn Runner>,
    /// `<data>/handover.json`.
    pub file: PathBuf,
    /// `<data>/handover/briefs`.
    pub briefs: PathBuf,
    /// Asks a process to exit. One SIGTERM in the app.
    pub stop: Arc<dyn Fn(i32) + Send + Sync>,
    /// How long the old session has to exit once asked.
    pub stop_wait: Duration,
}

impl HandoverHost {
    /// The app's own: the system runner, one SIGTERM and the office's stop wait (5 seconds).
    #[must_use]
    pub fn system(data: &Path) -> Self {
        Self {
            runner: Arc::new(SystemRunner),
            file: data.join("handover.json"),
            briefs: data.join("handover").join("briefs"),
            stop: Arc::new(crate::discovery::claude_code::terminate),
            stop_wait: STOP_WAIT,
        }
    }
}

/// Under the office lock.
#[derive(Default)]
pub(super) struct State {
    pub(super) book: Book,
    pub(super) works: WorkLog,
    /// Desks being handed over now, by session id.
    underway: BTreeSet<String>,
    /// Desks whose own proof was just read, with when: their automatic decision is still to take.
    undecided: BTreeMap<String, i64>,
    /// The terminal each handover of this run started the receiver in: terminal id to `Done::id`.
    /// Never saved: terminal ids start again with the app.
    started_in: BTreeMap<String, String>,
    /// What the last pass showed.
    pub(super) view: HandoverView,
    /// The Codex desks whose rollout counts: a top-level thread on the built-in provider (K5).
    codex_counts: BTreeSet<String>,
}

impl State {
    /// Record what the desks' own files said in one pass: which Codex desks count, their
    /// work, and each allowance that changed.
    fn note(&mut self, read: desk::Read, now: i64) {
        self.codex_counts = read
            .codex
            .iter()
            .filter(|(_, seen, _)| seen.counts)
            .map(|(id, _, _)| id.clone())
            .collect();
        for (id, seen, busy) in &read.codex {
            self.works.put(id, work::codex(&seen.goal, *busy));
        }
        for (id, tool, allowance) in read.changed {
            match allowance {
                Allowance::UsedUp {
                    window,
                    reset_at,
                    at,
                } => {
                    let record = UsedUp {
                        tool,
                        window,
                        reset_at: reset_at.filter(|r| *r <= now + RESET_HORIZON),
                        // Dated by the tool when it says (K3), never later than now.
                        seen_at: at.map_or(now, |t| t.min(now)),
                        session_id: id.clone(),
                    };
                    if record.live(now) && self.book.saw(record) {
                        self.undecided.insert(id, now);
                    }
                }
                Allowance::Clear => {
                    self.book.cleared(&id);
                    self.undecided.remove(&id);
                }
                Allowance::Unproven => {}
            }
        }
    }
}

/// The reader of each desk the building started, by its tool, with the last allowance acted on.
/// Taken before the office lock, never under it.
pub(super) type Limits = BTreeMap<String, (desk::Follower, Allowance)>;

/// The automatic decision waits this long after the proof is first read, so hook files still
/// being written are collected first.
const SETTLE_SECS: i64 = 2;

const NOT_SET_UP: &str = "handover is not set up";
const NOT_OURS: &str = "the building did not start this desk, so it cannot be handed over";
const NOT_USED_UP: &str = "this desk is not recorded as used up";
const FOLDER_GONE: &str = "that folder is gone";
const NO_ENTRY: &str = "this desk is not recorded as used up, or the building did not start it";

/// Nothing is chosen for the direction that ends in `to`.
fn choose(to: Tool) -> String {
    format!(
        "choose what {} starts with in Settings, under Handover, first",
        to.label()
    )
}

/// What one pass found: the handovers to start now, and whether the view changed.
#[derive(Default)]
pub(super) struct Due {
    pub(super) plans: Vec<Plan>,
    pub(super) changed: bool,
}

/// Everything a handover needs, gathered under the lock.
#[derive(Clone)]
pub(super) struct Plan {
    session_id: String,
    /// The floor the session is on, for a routed launch.
    project_id: String,
    terminal: String,
    by_human: bool,
    /// The record it answers: the desk's own, or its tool's live one.
    record: UsedUp,
    start: StartWith,
    title: String,
    folder: PathBuf,
    floor: String,
    branch: Option<String>,
    worktree: bool,
    /// What the work log said at that moment.
    work: Work,
}

/// Why a handover did not work, and whether the old session had already been ended.
struct Failure {
    reason: String,
    ended: bool,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Write the book when it changed. A failed save keeps the dirty mark, so the next pass retries.
fn save(host: &HandoverHost, book: &mut Book) {
    if book.dirty() && HandoverFile::new(host.file.clone()).save(book).is_ok() {
        book.saved();
    }
}

impl Office {
    /// Hand desks over with `host`. Its book is loaded now.
    #[must_use]
    pub fn with_handover(mut self, host: HandoverHost) -> Self {
        self.lock().handover.book = HandoverFile::new(host.file.clone()).load(now());
        self.handover_host = Some(host);
        self
    }

    fn handover_settings(&self) -> HandoverSettings {
        self.settings
            .as_ref()
            .map(|s| s.get().handover)
            .unwrap_or_default()
    }

    /// Whether `terminal` has a fixed role (a reviewer's, a manager's) a handover never touches.
    pub(super) fn fixed_role(inner: &Inner, terminal: &str) -> bool {
        inner.reviewers.contains_key(terminal)
            || inner.seats.values().any(|s| s.terminal == terminal)
    }

    /// The handover tool of the session `id` discovery found.
    fn tool_of(inner: &Inner, id: &str) -> Option<Tool> {
        inner
            .found
            .sessions
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| Tool::of(s.tool))
    }

    /// The desks a handover may reach, session id to terminal id: those `started` gives, less
    /// the Codex desks whose rollout does not count (K5).
    fn ours(&self, inner: &Inner) -> BTreeMap<String, String> {
        let mut ours = self.started(inner);
        ours.retain(|id, _| {
            Self::tool_of(inner, id) != Some(Tool::Codex)
                || inner.handover.codex_counts.contains(id)
        });
        ours
    }

    /// Every desk the building started in a terminal of its own that still runs, session id
    /// to terminal id: neither a reviewer's nor a manager's, under full control, of a tool the
    /// handover knows.
    fn started(&self, inner: &Inner) -> BTreeMap<String, String> {
        inner
            .owned
            .iter()
            .filter(|(terminal, session)| {
                !Self::fixed_role(inner, terminal)
                    && inner.found.sessions.iter().any(|s| {
                        &s.id == *session
                            && s.control == ControlMode::Full
                            && Tool::of(s.tool).is_some()
                    })
                    && matches!(self.terminals.wait_exit(terminal, Duration::ZERO), Ok(None))
            })
            .map(|(terminal, session)| (session.clone(), terminal.clone()))
            .collect()
    }

    /// The survey over every desk a handover knows (or `only` that one, with no sign-out lines).
    fn survey_of(
        &self,
        inner: &Inner,
        only: Option<&str>,
        fresh: &BTreeSet<String>,
        pending: &BTreeSet<String>,
    ) -> Survey {
        let ours = self.ours(inner);
        let settings = self.handover_settings();
        let desks: Vec<DeskFacts<'_>> = inner
            .found
            .sessions
            .iter()
            .filter(|s| only.is_none_or(|id| id == s.id))
            .filter_map(|s| {
                Tool::of(s.tool).map(|tool| DeskFacts {
                    session_id: &s.id,
                    tool,
                    ours: ours.contains_key(&s.id),
                })
            })
            .collect();
        let ended: Vec<&str> = match only {
            Some(_) => Vec::new(),
            None => inner.found.ended.iter().map(|e| e.id.as_str()).collect(),
        };
        let state = &inner.handover;
        survey(
            &state.book,
            &state.works,
            &desks,
            &Now {
                now: now(),
                settings: &settings,
                fresh,
                pending,
                underway: &state.underway,
                ended: &ended,
            },
        )
    }

    /// Show what each desk has to say now, deciding nothing automatically.
    fn resurvey(&self, inner: &mut Inner) {
        let pending: BTreeSet<String> = inner.handover.undecided.keys().cloned().collect();
        inner.handover.view = self.survey_of(inner, None, &BTreeSet::new(), &pending).view;
    }

    /// One pass: collect what the hooks and the transcripts say, record what is proven, and
    /// return the desks to hand over now. Called by the watch loop only.
    /// A session that did not stop when asked counts as ended only once the terminal it last
    /// ran in has exited or closed: one that merely drops out of discovery may still run.
    fn note_ended_late(&self, inner: &mut Inner) {
        let ran_in = &inner.ran_in;
        let terminals = &self.terminals;
        for from in [Tool::ClaudeCode, Tool::Codex] {
            inner.handover.book.ended_late(
                &run::did_not_stop(from),
                &run::ended_late(from),
                |id| {
                    ran_in.get(id).is_some_and(|terminal| {
                        !matches!(terminals.wait_exit(terminal, Duration::ZERO), Ok(None))
                    })
                },
            );
        }
    }

    pub(super) fn handover_due(&self) -> Due {
        let Some(host) = &self.handover_host else {
            return Due::default();
        };
        let now = now();
        let mut limits = lock(&self.limits);
        // Each desk the building started: its session id, its tool and, for Codex, how many of
        // its sub-agent threads have a turn in flight.
        let desks: Vec<desk::Started> = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            for signal in inner.feed.take_signals() {
                inner.handover.works.note(signal);
            }
            let live: BTreeSet<&str> = inner.found.sessions.iter().map(|s| s.id.as_str()).collect();
            // A new desk has no session yet, and its first listing must outlive that wait.
            inner
                .handover
                .works
                .keep(|id| live.contains(id), std::time::SystemTime::now());
            self.started(inner)
                .into_keys()
                .filter_map(|id| {
                    let tool = Self::tool_of(inner, &id)?;
                    let busy = match tool {
                        Tool::ClaudeCode => 0,
                        Tool::Codex => inner.discovery.codex_subagents_busy(&id),
                    };
                    Some((id, tool, busy))
                })
                .collect()
        };
        // No office lock: the transcripts and rollouts are read here.
        let read = desk::follow(&self.home, &mut limits, &desks);
        let mut guard = self.lock();
        let inner = &mut *guard;
        let state = &mut inner.handover;
        state.note(read, now);
        state.book.prune(now);
        for (terminal, done) in &state.started_in {
            if let Some(session) = inner.owned.get(terminal) {
                state.book.bound(done, session);
            }
        }
        let ours = self.ours(inner);
        self.note_ended_late(inner);
        let state = &mut inner.handover;
        state.undecided.retain(|id, _| ours.contains_key(id));
        let fresh: BTreeSet<String> = state
            .undecided
            .iter()
            .filter(|(_, at)| now - **at >= SETTLE_SECS)
            .map(|(id, _)| id.clone())
            .collect();
        state.undecided.retain(|id, _| !fresh.contains(id));
        let pending: BTreeSet<String> = state.undecided.keys().cloned().collect();
        let found = self.survey_of(inner, None, &fresh, &pending);
        let mut plans = Vec::new();
        for id in found.hand_over {
            if let Ok(plan) = self.plan(inner, &id, false) {
                inner.handover.underway.insert(id);
                plans.push(plan);
            }
        }
        // Again with the decisions taken, so a plan that could not be built shows no "underway".
        let view = self.survey_of(inner, None, &BTreeSet::new(), &pending).view;
        let changed = view != inner.handover.view;
        inner.handover.view = view;
        save(host, &mut inner.handover.book);
        Due { plans, changed }
    }

    /// Everything a handover of `session_id` needs, or why it cannot be had.
    fn plan(&self, inner: &Inner, session_id: &str, by_human: bool) -> Result<Plan, String> {
        let ours = self.ours(inner);
        let terminal = ours.get(session_id).ok_or(NOT_OURS)?;
        let session = inner
            .found
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .ok_or(NOT_OURS)?;
        let tool = Tool::of(session.tool).ok_or(NOT_OURS)?;
        let now = now();
        let book = &inner.handover.book;
        let record = book
            .own(session_id, now)
            .or_else(|| book.live(tool, now))
            .cloned()
            .ok_or(NOT_USED_UP)?;
        let start = self
            .handover_settings()
            .start_with(tool)
            .cloned()
            .ok_or_else(|| choose(tool.other()))?;
        let folder = inner
            .found
            .checkouts
            .get(session_id)
            .or_else(|| inner.found.roots.get(&session.project_id))
            .cloned()
            .ok_or(FOLDER_GONE)?;
        let floor = inner
            .found
            .projects
            .iter()
            .find(|p| p.id == session.project_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        Ok(Plan {
            session_id: session_id.to_string(),
            project_id: session.project_id.clone(),
            terminal: terminal.clone(),
            by_human,
            record,
            start,
            title: session.title.clone(),
            folder,
            floor,
            branch: session.branch.clone(),
            worktree: session.worktree,
            work: inner.handover.works.of(session_id),
        })
    }

    /// A Human asked for this desk to be handed over: allowed only where the building would
    /// offer it. Ends the desk's session, and with it any workflow it runs.
    pub fn hand_over_desk(&self, session_id: &str) -> Result<(), String> {
        if self.handover_host.is_none() {
            return Err(NOT_SET_UP.to_string());
        }
        let plan = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            if inner.handover.underway.contains(session_id) {
                return Err("this desk is already being handed over".to_string());
            }
            let none = BTreeSet::new();
            let state = self
                .survey_of(inner, Some(session_id), &none, &none)
                .view
                .desks
                .into_iter()
                .find(|d| d.session_id == session_id)
                .map(|d| d.state);
            match state {
                Some(DeskState::Offered { .. }) => {}
                Some(DeskState::Blocked { to, why, .. }) => {
                    return Err(match why {
                        BlockWhy::NotChosen => choose(to),
                        BlockWhy::BothUsedUp => {
                            format!(
                                "{} is used up too, so this desk stays where it is",
                                to.label()
                            )
                        }
                        BlockWhy::AlreadyHandedOver => {
                            "this desk was already handed over for this limit".to_string()
                        }
                    });
                }
                Some(
                    DeskState::Underway { .. }
                    | DeskState::Arrived { .. }
                    | DeskState::Left { .. }
                    | DeskState::Failed { .. },
                )
                | None => return Err(NO_ENTRY.to_string()),
            }
            let plan = self.plan(inner, session_id, true)?;
            inner.handover.underway.insert(session_id.to_string());
            plan
        };
        self.carry_out_handover(&plan)
    }

    /// Record how a handover went, take the desk out of `underway`, save and show it.
    /// `result`: the receiver's terminal, or why it did not work and whether the old session
    /// had been ended.
    fn finish(
        &self,
        plan: &Plan,
        id: String,
        result: Result<String, Failure>,
    ) -> Result<(), String> {
        let mut guard = self.lock();
        let inner = &mut *guard;
        let state = &mut inner.handover;
        let (outcome, answer) = match result {
            Ok(terminal) => {
                state.started_in.insert(terminal, id.clone());
                (Outcome::Started { session: None }, Ok(()))
            }
            Err(Failure { reason, ended }) => (
                Outcome::Failed {
                    reason: reason.clone(),
                    ended,
                },
                Err(reason),
            ),
        };
        state.book.finished(Done {
            id,
            from_session: plan.session_id.clone(),
            from: plan.record.tool,
            to: plan.record.tool.other(),
            at: now(),
            window: plan.record.window,
            reset_at: plan.record.reset_at,
            seen_at: plan.record.seen_at,
            outcome,
        });
        state.underway.remove(&plan.session_id);
        if let Some(host) = &self.handover_host {
            save(host, &mut state.book);
        }
        self.resurvey(inner);
        answer
    }
}

/// Run one pass and start what it found due, each on its own thread, calling `done` after
/// each. True when the view changed.
pub fn start_due_handovers(office: &Arc<Office>, done: impl Fn() + Send + Sync + 'static) -> bool {
    let due = office.handover_due();
    let done = Arc::new(done);
    let mut changed = due.changed;
    for plan in due.plans {
        let kept = plan.clone();
        let (office_ref, done) = (Arc::clone(office), Arc::clone(&done));
        let spawned = std::thread::Builder::new()
            .name("handover".to_string())
            .spawn(move || {
                // The outcome is recorded and shown by `carry_out_handover` itself.
                let _ = office_ref.carry_out_handover(&plan);
                done();
            });
        if spawned.is_err() {
            let id = office.lock().handover.book.next_id();
            let failed = Failure {
                reason: "the building could not start the handover".to_string(),
                ended: false,
            };
            // The reason is recorded and shown on the desk.
            let _ = office.finish(&kept, id, Err(failed));
            changed = true;
        }
    }
    changed
}
