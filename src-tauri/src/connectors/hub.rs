//! The cache between the connectors and the snapshot. The poller fills it off the refresh
//! path, the snapshot reads it under a short lock, and a tick shows here first.
//!
//! Lock order: the Office lock, then this hub's lock, never the other way. The hub calls
//! `changed` and every connector with no lock of its own held.

use super::{Connector, ConnectorError, Item, LinkedFloor, Listing, item_id};
use crate::model::{Connection, ConnectorKind, Link, TodoItem, TodoSource};
use crate::poll::{Outcome, Poller, Schedule};
use crate::sync::lock;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const GONE: &str = "that issue is no longer on this floor's list";
/// The poll interval when no connector is registered.
const DEFAULT_EVERY: Duration = Duration::from_secs(60);
/// The longest wait after repeated unreachable passes.
const MAX_BACKOFF: Duration = Duration::from_mins(5);

/// A board's key: (repo root, connector).
type Key = (String, ConnectorKind);

pub struct Hub {
    connectors: Vec<Arc<dyn Connector>>,
    state: Mutex<State>,
    /// Called with no lock held, after anything a snapshot shows has changed.
    changed: Box<dyn Fn() + Send + Sync>,
    poller: OnceLock<Poller>,
}

#[derive(Default)]
struct State {
    boards: BTreeMap<Key, Board>,
    /// Counts poll passes and confirmed ticks, to order them.
    seq: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Board {
    /// The last good listing.
    items: Vec<Item>,
    truncated: bool,
    fetched_at: Option<String>,
    error: Option<String>,
    needs_key: bool,
    /// Ticks in flight, by external id: shown as done.
    ticking: BTreeSet<String>,
    /// Items ticked here, with `seq` at the moment the source confirmed: shown as done until
    /// a listing that began later replaces them.
    ticked: Vec<(u64, Item)>,
}

impl Board {
    /// Keep what a listing that began at `began` found.
    fn apply(
        &mut self,
        began: u64,
        result: Result<Listing, ConnectorError>,
        kind: ConnectorKind,
        now: &str,
    ) {
        match result {
            Ok(listing) => {
                self.ticked.retain(|(at, _)| *at > began);
                let ticked = &self.ticked;
                self.items = listing
                    .items
                    .into_iter()
                    .filter(|i| !ticked.iter().any(|(_, t)| t.external_id == i.external_id))
                    .collect();
                self.truncated = listing.truncated;
                self.fetched_at = Some(now.to_string());
                self.error = None;
                self.needs_key = false;
            }
            Err(e @ ConnectorError::NoKey(_)) => {
                self.items.clear();
                self.ticked.clear();
                self.fetched_at = None;
                self.truncated = false;
                self.needs_key = true;
                self.error = Some(e.words(kind));
            }
            Err(e) => {
                self.error = Some(e.words(kind));
                self.needs_key = false;
            }
        }
    }

    /// The item with this external id, open or ticked here.
    fn find(&self, external_id: &str) -> Option<&Item> {
        self.items
            .iter()
            .chain(self.ticked.iter().map(|(_, i)| i))
            .find(|i| i.external_id == external_id)
    }
}

fn todo(project_id: &str, kind: ConnectorKind, item: &Item, done: bool) -> TodoItem {
    TodoItem {
        id: item_id(project_id, kind, &item.external_id),
        project_id: project_id.to_string(),
        text: item.text.clone(),
        done,
        assignee: item.assignee,
        source: item.source.clone(),
    }
}

impl Hub {
    #[must_use]
    pub fn new(connectors: Vec<Arc<dyn Connector>>, changed: Box<dyn Fn() + Send + Sync>) -> Self {
        Self {
            connectors,
            state: Mutex::new(State::default()),
            changed,
            poller: OnceLock::new(),
        }
    }

    fn connector(&self, kind: ConnectorKind) -> Option<&Arc<dyn Connector>> {
        self.connectors.iter().find(|c| c.kind() == kind)
    }

    /// One pass: ask each linked floor's connectors, outside every lock, and keep the answers.
    pub fn poll(&self, floors: &[LinkedFloor], now: &str) -> Outcome {
        let linked: BTreeSet<Key> = floors
            .iter()
            .flat_map(|f| f.links.iter().map(|l| (f.root.clone(), l.kind())))
            .collect();
        let (began, before) = {
            let mut state = lock(&self.state);
            state.seq += 1;
            // Taken before the pruning, so a dropped board counts as a change.
            let before = state.boards.clone();
            state.boards.retain(|key, _| linked.contains(key));
            for key in &linked {
                state.boards.entry(key.clone()).or_default();
            }
            (state.seq, before)
        };
        let mut failed = false;
        let mut held = None;
        'floors: for floor in floors {
            for link in &floor.links {
                let kind = link.kind();
                let Some(connector) = self.connector(kind) else {
                    continue;
                };
                let result = connector.list(link);
                match &result {
                    Err(ConnectorError::RateLimited(wait)) => {
                        held = Some(wait.unwrap_or_else(|| connector.poll_interval()));
                    }
                    Err(ConnectorError::Unreachable(_)) => failed = true,
                    _ => {}
                }
                if let Some(board) = lock(&self.state)
                    .boards
                    .get_mut(&(floor.root.clone(), kind))
                {
                    board.apply(began, result, kind, now);
                }
                if held.is_some() {
                    break 'floors;
                }
            }
        }
        if lock(&self.state).boards != before {
            (self.changed)();
        }
        match held {
            Some(wait) => Outcome::RetryAfter(wait),
            None if failed => Outcome::Failed,
            None => Outcome::Ok,
        }
    }

    /// A floor's connector items as TODO items: the listing, then what was ticked here.
    #[must_use]
    pub fn todos(&self, project_id: &str, root: &str) -> Vec<TodoItem> {
        let state = lock(&self.state);
        state
            .boards
            .iter()
            .filter(|((r, _), _)| r == root)
            .flat_map(|((_, kind), board)| {
                let open = board
                    .items
                    .iter()
                    .map(|i| todo(project_id, *kind, i, board.ticking.contains(&i.external_id)));
                let ticked = board
                    .ticked
                    .iter()
                    .map(|(_, i)| todo(project_id, *kind, i, true));
                open.chain(ticked).collect::<Vec<_>>()
            })
            .collect()
    }

    /// How a floor's link stands. A link never checked yet: no time, no error.
    #[must_use]
    pub fn connection(&self, project_id: &str, root: &str, link: &Link) -> Connection {
        let state = lock(&self.state);
        let board = state.boards.get(&(root.to_string(), link.kind()));
        Connection {
            project_id: project_id.to_string(),
            link: link.clone(),
            fetched_at: board.and_then(|b| b.fetched_at.clone()),
            error: board.and_then(|b| b.error.clone()),
            needs_key: board.is_some_and(|b| b.needs_key),
            truncated: board.is_some_and(|b| b.truncated),
        }
    }

    /// Mark an item done: shown at once, sent to the source, rolled back if it refuses.
    pub fn tick(&self, root: &str, link: &Link, external_id: &str) -> Result<(), String> {
        let kind = link.kind();
        let connector = self
            .connector(kind)
            .ok_or("that connector is not available")?;
        let key = (root.to_string(), kind);
        {
            let mut state = lock(&self.state);
            let board = state
                .boards
                .get_mut(&key)
                .filter(|b| b.items.iter().any(|i| i.external_id == external_id))
                .ok_or(GONE)?;
            if !board.ticking.insert(external_id.to_string()) {
                return Err("that item is already being marked done".to_string());
            }
        }
        (self.changed)();
        let result = connector.tick(link, external_id);
        {
            let mut state = lock(&self.state);
            state.seq += 1;
            let seq = state.seq;
            if let Some(board) = state.boards.get_mut(&key) {
                board.ticking.remove(external_id);
                if result.is_ok()
                    && let Some(at) = board
                        .items
                        .iter()
                        .position(|i| i.external_id == external_id)
                {
                    let item = board.items.remove(at);
                    board.ticked.push((seq, item));
                }
            }
        }
        (self.changed)();
        result.map_err(|e| e.words(kind))
    }

    /// An item's page at its source, if it has one.
    #[must_use]
    pub fn url(&self, root: &str, kind: ConnectorKind, external_id: &str) -> Option<String> {
        let state = lock(&self.state);
        let item = state
            .boards
            .get(&(root.to_string(), kind))?
            .find(external_id)?;
        match &item.source {
            TodoSource::Linear { url, .. } => Some(url.clone()),
            TodoSource::App | TodoSource::File { .. } => None,
        }
    }

    /// Drop a floor's board (it was disconnected).
    pub fn forget(&self, root: &str, kind: ConnectorKind) {
        lock(&self.state).boards.remove(&(root.to_string(), kind));
    }

    /// Poll now. Nothing happens before `start`.
    pub fn wake(&self) {
        if let Some(poller) = self.poller.get() {
            poller.wake();
        }
    }

    /// The human asked: let every connector forget a refusal, then poll now.
    pub fn recheck(&self) {
        for connector in &self.connectors {
            connector.recheck();
        }
        self.wake();
    }
}

/// Start the poller thread ("connector-poll"). `floors` is asked at the start of every pass.
pub fn start(
    hub: &Arc<Hub>,
    floors: impl Fn() -> Vec<LinkedFloor> + Send + 'static,
) -> Result<(), String> {
    let every = hub
        .connectors
        .iter()
        .map(|c| c.poll_interval())
        .min()
        .unwrap_or(DEFAULT_EVERY);
    let schedule = Schedule {
        every,
        max_backoff: MAX_BACKOFF,
    };
    let polled = Arc::clone(hub);
    let running = Poller::start("connector-poll", schedule, move || {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        polled.poll(&floors(), &now)
    })?;
    hub.poller
        .set(running)
        .map_err(|_| "the connector poller is already running".to_string())
}

#[cfg(test)]
mod tests;
