//! A connector for tests: plays queued listings, records its calls, and can run a one-shot
//! hook in the middle of a `list` or a `tick`.

use super::{Connector, ConnectorError, Item, Listing};
use crate::model::{ConnectorKind, Link, Party, TodoSource};
use crate::sync::lock;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

type Hook = Box<dyn FnOnce() + Send>;
type Answer = Result<Listing, ConnectorError>;

pub struct FakeConnector {
    every: Duration,
    /// Played in order, then the last one played again. Nothing ever queued: an empty listing.
    lists: Mutex<(VecDeque<Answer>, Option<Answer>)>,
    tick_result: Mutex<Result<(), ConnectorError>>,
    listed: Mutex<Vec<Link>>,
    ticked: Mutex<Vec<(Link, String)>>,
    rechecks: Mutex<usize>,
    on_list: Mutex<Option<Hook>>,
    on_tick: Mutex<Option<Hook>>,
}

impl FakeConnector {
    #[must_use]
    pub fn new(every: Duration) -> Self {
        Self {
            every,
            lists: Mutex::new((VecDeque::new(), None)),
            tick_result: Mutex::new(Ok(())),
            listed: Mutex::new(Vec::new()),
            ticked: Mutex::new(Vec::new()),
            rechecks: Mutex::new(0),
            on_list: Mutex::new(None),
            on_tick: Mutex::new(None),
        }
    }

    pub fn push_list(&self, result: Answer) {
        lock(&self.lists).0.push_back(result);
    }

    pub fn set_tick(&self, result: Result<(), ConnectorError>) {
        *lock(&self.tick_result) = result;
    }

    /// Run `hook` once, inside the next `list`, before it answers.
    pub fn on_list(&self, hook: impl FnOnce() + Send + 'static) {
        *lock(&self.on_list) = Some(Box::new(hook));
    }

    /// Run `hook` once, inside the next `tick`, before it answers.
    pub fn on_tick(&self, hook: impl FnOnce() + Send + 'static) {
        *lock(&self.on_tick) = Some(Box::new(hook));
    }

    #[must_use]
    pub fn listed(&self) -> Vec<Link> {
        lock(&self.listed).clone()
    }

    #[must_use]
    pub fn ticked(&self) -> Vec<(Link, String)> {
        lock(&self.ticked).clone()
    }

    #[must_use]
    pub fn rechecks(&self) -> usize {
        *lock(&self.rechecks)
    }
}

/// Take a one-shot hook out of its slot and run it with no lock held.
fn run(slot: &Mutex<Option<Hook>>) {
    let hook = lock(slot).take();
    if let Some(hook) = hook {
        hook();
    }
}

impl Connector for FakeConnector {
    fn kind(&self) -> ConnectorKind {
        ConnectorKind::Linear
    }

    fn poll_interval(&self) -> Duration {
        self.every
    }

    fn list(&self, link: &Link) -> Result<Listing, ConnectorError> {
        lock(&self.listed).push(link.clone());
        run(&self.on_list);
        let mut lists = lock(&self.lists);
        let (queue, last) = &mut *lists;
        if let Some(next) = queue.pop_front() {
            *last = Some(next);
        }
        last.clone().unwrap_or_else(|| Ok(Listing::default()))
    }

    fn tick(&self, link: &Link, external_id: &str) -> Result<(), ConnectorError> {
        lock(&self.ticked).push((link.clone(), external_id.to_string()));
        run(&self.on_tick);
        lock(&self.tick_result).clone()
    }

    fn recheck(&self) {
        *lock(&self.rechecks) += 1;
    }
}

/// A Linear issue with invented data: `ENG-<n>` with id `issue-<n>`.
#[must_use]
pub fn issue(n: u32, assignee: Party) -> Item {
    let identifier = format!("ENG-{n}");
    Item {
        external_id: format!("issue-{n}"),
        text: format!("Issue {n}"),
        assignee,
        source: TodoSource::Linear {
            url: format!("https://linear.app/example/issue/{identifier}"),
            identifier,
        },
    }
}

/// A listing of `items`, not truncated.
#[must_use]
pub fn listing(items: Vec<Item>) -> Listing {
    Listing {
        items,
        truncated: false,
    }
}
