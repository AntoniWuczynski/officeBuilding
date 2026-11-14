//! The router: the routed sessions and their secrets, the one route lock, the exhaustion file,
//! credentials, retry, switch and the view. `relay.rs` answers the requests.
//!
//! `RouteState` is only ever reached through `with_state` (and `apply_preferences`, which is
//! memory only). A closure given to `with_state` must be short and pure: no Keychain, no file,
//! no network, no sleep and no other lock.

mod relay;

use super::answers;
use super::config::{ConnectionId, ConnectionKind, ContextId, Targets, Tiers};
use super::keys::{KeyStatus, Keys};
use super::launch;
use super::live::{DeskRoute, Reasoning};
use super::policy::{RouteError, SessionRoute};
use super::records::{ExhaustionFile, Saved};
use super::serve;
use super::state::{RouteId, RouteState};
use super::upstream::{self, Bases, Upstream};
use crate::manager::auth::new_token;
use crate::secrets::Secret;
use crate::settings::RoutingSettings;
use crate::sync::lock;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Characters of a route id: the first half of one token.
const ID_LEN: usize = 32;
pub(crate) const NOT_ROUTED: &str = "that desk is not routed";
pub(crate) const NOT_IN_ORDER: &str = "that connection is not in this desk's order";

/// What a routed launch is given. Deriving `Debug` is safe only because `header` is a `Secret`,
/// which prints as `Secret(..)`. Keep it one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub id: RouteId,
    /// `http://127.0.0.1:<port>/s/<route id>`.
    pub base_url: String,
    /// The whole value of `ANTHROPIC_CUSTOM_HEADERS`: `x-office-building-route: <secret>`.
    pub header: Secret,
}

/// One entry of a session's order, copied when it was opened: enough to work out credentials,
/// to word an answer and to map a per-message effort (F12) without taking the route lock.
/// Never updated, like the copy Stage A keeps (D10).
struct Link {
    connection: ConnectionId,
    name: String,
    kind: ConnectionKind,
    targets: Targets,
}

struct Routed {
    secret: Secret,
    chain: Vec<Link>,
    /// The session's own copy of the tiers, taken when it was opened.
    tiers: Tiers,
    /// It was started with the tool's resume command, so its history is not this route's own.
    resumed: bool,
    live: Mutex<Live>,
}

impl Routed {
    fn link(&self, connection: &ConnectionId) -> Option<&Link> {
        self.chain
            .iter()
            .find(|link| link.connection == *connection)
    }
}

#[derive(Default)]
struct Live {
    terminal: Option<String>,
    /// Connections whose reply bytes have reached this desk.
    spoke: BTreeSet<ConnectionId>,
    /// Messages requests of this desk the router has taken in (read, and put to `begin`).
    requests: u64,
}

pub struct Router {
    state: Mutex<RouteState>,
    file: ExhaustionFile,
    keys: Arc<Keys>,
    upstream: Arc<dyn Upstream>,
    bases: Bases,
    sessions: Mutex<BTreeMap<RouteId, Arc<Routed>>>,
    /// The listener's port, once it is listening.
    port: Mutex<Option<u16>>,
    /// Why the exhaustion records could not be read or written, for the status.
    records_problem: Mutex<Option<String>>,
}

/// Unix seconds, 0 when the clock is before the epoch. Read outside the route lock.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

impl Router {
    /// Loads the exhaustion file and builds the state from its records.
    #[must_use]
    pub fn new(
        file: ExhaustionFile,
        keys: Arc<Keys>,
        upstream: Arc<dyn Upstream>,
        bases: Bases,
    ) -> Arc<Self> {
        let loaded = file.load();
        Arc::new(Self {
            state: Mutex::new(RouteState::new(loaded.records)),
            file,
            keys,
            upstream,
            bases,
            sessions: Mutex::new(BTreeMap::new()),
            port: Mutex::new(None),
            records_problem: Mutex::new(loaded.problem),
        })
    }

    /// Open a session for a desk about to be launched on a floor of `context`.
    /// Ok(None): this launch is not routed (the switch is off, the context is gone, its order is empty).
    ///
    /// # Errors
    /// It should be routed and cannot be (E16): the listener did not start, or the system gave
    /// no random bytes.
    pub fn open(
        self: &Arc<Self>,
        settings: &RoutingSettings,
        context: &ContextId,
        resumed: bool,
    ) -> Result<Option<Opened>, String> {
        let Ok(route) = SessionRoute::open(settings, context) else {
            return Ok(None);
        };
        let port = self.listening()?;
        let id = RouteId::new(new_token()?.expose().chars().take(ID_LEN).collect());
        let secret = new_token()?;
        let chain = route
            .entries
            .iter()
            .map(|entry| Link {
                connection: entry.connection.clone(),
                name: entry.name.clone(),
                kind: entry.kind,
                targets: entry.targets.clone(),
            })
            .collect();
        let tiers = route.tiers.clone();
        self.with_state(|s| s.open(id.clone(), route));
        let header = launch::header_line(&secret);
        lock(&self.sessions).insert(
            id.clone(),
            Arc::new(Routed {
                secret,
                chain,
                tiers,
                resumed,
                live: Mutex::new(Live::default()),
            }),
        );
        Ok(Some(Opened {
            base_url: launch::base_url(port, &id),
            id,
            header,
        }))
    }

    /// The listener's port, starting it the first time (E1).
    fn listening(self: &Arc<Self>) -> Result<u16, String> {
        let mut port = lock(&self.port);
        if let Some(port) = *port {
            return Ok(port);
        }
        let handler: Arc<dyn serve::Handler> = Arc::<Self>::clone(self);
        let started = serve::listen(handler)?;
        *port = Some(started);
        Ok(started)
    }

    fn routed(&self, id: &RouteId) -> Option<Arc<Routed>> {
        lock(&self.sessions).get(id).cloned()
    }

    /// The terminal the session's desk runs in.
    pub fn bind(&self, id: &RouteId, terminal: &str) {
        if let Some(routed) = self.routed(id) {
            lock(&routed.live).terminal = Some(terminal.to_owned());
        }
    }

    /// The registry entry goes first, so no new request is admitted while the state closes it.
    pub fn close(&self, id: &RouteId) {
        lock(&self.sessions).remove(id);
        self.with_state(|s| s.close(id));
    }

    /// Close the session bound to `terminal`, if there is one.
    pub fn terminal_exited(&self, terminal: &str) {
        if let Some(id) = self.route_of(terminal) {
            self.close(&id);
        }
    }

    #[must_use]
    pub fn route_of(&self, terminal: &str) -> Option<RouteId> {
        let sessions: Vec<(RouteId, Arc<Routed>)> = lock(&self.sessions)
            .iter()
            .map(|(id, routed)| (id.clone(), Arc::clone(routed)))
            .collect();
        sessions
            .into_iter()
            .find(|(_, routed)| lock(&routed.live).terminal.as_deref() == Some(terminal))
            .map(|(id, _)| id)
    }

    /// May read the Keychain: call it off the main thread.
    #[must_use]
    pub fn view(&self, id: &RouteId) -> Option<DeskRoute> {
        let routed = self.routed(id)?;
        let credentials = self.credentials(&routed.chain);
        let now = now();
        let status = self.with_state(|s| s.status(id, &credentials, now))?;
        let active = status
            .active
            .as_ref()
            .and_then(|c| routed.link(c))
            .map(|link| (&link.connection, link.kind));
        let (reasoning, requests) = {
            let live = lock(&routed.live);
            (
                Reasoning::of(routed.resumed, &live.spoke, active),
                live.requests,
            )
        };
        let records_problem = lock(&self.records_problem).clone();
        Some(DeskRoute {
            status,
            reasoning,
            requests,
            records_problem,
        })
    }

    /// Arm one check of an exhausted connection in this desk's own order (E20).
    ///
    /// # Errors
    /// The desk is not routed, the connection is not in its order, or it has no record.
    pub fn retry(&self, id: &RouteId, connection: &ConnectionId) -> Result<DeskRoute, String> {
        let routed = self.routed(id).ok_or(NOT_ROUTED)?;
        routed.link(connection).ok_or(NOT_IN_ORDER)?;
        let now = now();
        if !self.with_state(|s| s.retry(connection, now)) {
            return Err(
                "that connection has no used-up allowance on record, so there is nothing to retry"
                    .to_owned(),
            );
        }
        self.view(id).ok_or_else(|| NOT_ROUTED.to_owned())
    }

    /// Move this desk to another entry of its own order.
    ///
    /// # Errors
    /// The desk is not routed, the connection is not in its order, or it cannot be used now.
    pub fn switch(&self, id: &RouteId, connection: &ConnectionId) -> Result<DeskRoute, String> {
        let routed = self.routed(id).ok_or(NOT_ROUTED)?;
        let credentials = self.credentials(&routed.chain);
        let now = now();
        self.with_state(|s| s.switch(id, connection, &credentials, now))
            .map_err(|error| match error {
                RouteError::UnknownSession => NOT_ROUTED.to_owned(),
                RouteError::NotInChain => NOT_IN_ORDER.to_owned(),
                RouteError::Refused { connection, reason } => match routed.link(&connection) {
                    Some(link) => format!(
                        "{} cannot be switched to, because {}",
                        link.name,
                        answers::skip_sentence(link.kind, &reason)
                    ),
                    None => "that connection cannot be switched to".to_owned(),
                },
                RouteError::Blocked { .. } => "that connection cannot be switched to".to_owned(),
            })?;
        self.view(id).ok_or_else(|| NOT_ROUTED.to_owned())
    }

    /// Settings changed: narrow every session and drop records of connections that are gone.
    /// Memory only, so it may run on the main thread: `flush` writes what changed.
    pub fn apply_preferences(&self, settings: &RoutingSettings) {
        lock(&self.state).apply_preferences(settings);
    }

    /// Write the records if they changed. May block on the file.
    pub fn flush(&self) {
        self.with_state(|_| ());
    }

    /// The connections of `chain` the building holds a credential for now, among the kinds this
    /// stage relays (E13). May read the Keychain and wait on a system prompt.
    fn credentials(&self, chain: &[Link]) -> BTreeSet<ConnectionId> {
        chain
            .iter()
            .filter(|link| {
                upstream::relays(link.kind)
                    && link.kind.holds_key()
                    && self.keys.status(link.kind, &link.connection) == KeyStatus::Saved
            })
            .map(|link| link.connection.clone())
            .collect()
    }

    /// Run `f` under the route lock, then write what changed and confirm it.
    /// Nothing else is locked while `f` runs, and the lock is not held for the write.
    fn with_state<R>(&self, f: impl FnOnce(&mut RouteState) -> R) -> R {
        let (out, unsaved) = {
            let mut state = lock(&self.state);
            let out = f(&mut state);
            (out, state.unsaved())
        };
        self.save(unsaved);
        out
    }

    /// A failed write is kept for the status and offered again by the next `unsaved`.
    fn save(&self, unsaved: Option<Saved>) {
        let Some(saved) = unsaved else {
            return;
        };
        match self.file.save(&saved) {
            Ok(()) => {
                lock(&self.state).saved(saved.revision);
                *lock(&self.records_problem) = None;
            }
            Err(why) => {
                *lock(&self.records_problem) = Some(format!(
                    "The routing exhaustion records could not be saved: {why}"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests;
