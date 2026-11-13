//! The failover state machine: routed sessions, attempt leases, the blocked state, explicit
//! retry and switch. Pure and single-threaded: no network, no thread, no file, no clock.
//! Time is a `u64` of Unix seconds passed in by the caller. See the module doc of `routing`
//! for how Stage B must call it.
//!
//! The no-loop rules kept here: a session moves only when its active entry is exhausted, and
//! only once per generation (compare-and-swap); a lease of an older generation never moves a
//! session or clears a record; a transient failure is retried at most once within a small
//! budget and never counts as exhaustion; nothing is retried or moved on once a byte of the
//! reply has been relayed, or after a dropped connection.

use super::availability::{Availability, Bucket, ExhaustionRecord, Ticket};
use super::capabilities::Requirements;
use super::config::{ConnectionId, ConnectionKind};
use super::observe::{Breadth, Observation};
use super::policy::{
    Attempt, Needs, RouteError, SessionRoute, SessionView, Skip, Target, fits_session,
    plan_attempt, standing,
};
use super::records::Saved;
use super::status::{
    EntryState, EntryStatus, ExhaustedScope, Problem, ProblemKind, RouteStatus, Served, Switch,
    SwitchReason,
};
use crate::settings::RoutingSettings;
use std::collections::{BTreeMap, BTreeSet};

/// A routed session's id. Correlation only: Stage B authenticates with a separate secret.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RouteId(String);

impl RouteId {
    #[must_use]
    pub fn new(id: String) -> Self {
        Self(id)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The right to make one upstream call. Fixed when it is cut: nothing that happens later changes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub id: u64,
    pub session: RouteId,
    /// The session's generation when this was cut.
    pub generation: u64,
    pub entry: usize,
    pub connection: ConnectionId,
    pub kind: ConnectionKind,
    pub target: Target,
    tickets: Vec<Ticket>,
    request: Requirements,
    attempt: Attempt,
    /// The request's bucket under the session's tiers when this was cut.
    class: Bucket,
}

impl Lease {
    #[must_use]
    pub fn is_probe(&self) -> bool {
        !self.tickets.is_empty()
    }
}

/// A local answer for which no upstream reply exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFault {
    /// The upstream was never reached. Nothing ran.
    Unreachable,
    /// The connection broke after the request was sent. It may have run.
    Interrupted,
}

/// What the caller does next with the request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "one verdict per upstream attempt, consumed at once: boxing the lease buys nothing"
)]
pub enum Verdict {
    /// Nothing more.
    Done,
    /// Call upstream again with this lease, after `wait_ms` (0: at once).
    Again { lease: Lease, wait_ms: u64 },
    /// Give the client the upstream's own answer: its status, headers and body, unchanged.
    Relay,
    /// There is no upstream answer to give: answer locally, truthfully.
    Local(LocalFault),
    /// Bytes of the reply have already gone to the client: end the stream with no clean terminator. Never replay.
    Cut,
}

/// Times one request may ask the same entry again after a transient failure.
pub const RETRIES_MAX: u8 = 1;
/// A retry is only cut this soon after the request's first attempt.
pub const ATTEMPT_WINDOW_SECS: u64 = 30;
/// The wait before a retry when the provider gave none.
pub const RETRY_WAIT_MS: u64 = 500;
/// A provider's wait longer than this is not waited out: the refusal is relayed.
pub const WAIT_MAX_MS: u64 = 5_000;

/// One routed session. In memory only.
#[derive(Debug)]
struct Session {
    route: SessionRoute,
    active: Option<usize>,
    /// Goes up by one every time `active` changes.
    generation: u64,
    needs: Needs,
    blocked_since: Option<u64>,
    last_switch: Option<Switch>,
    last_served: Option<Served>,
}

impl Session {
    fn active_connection(&self) -> Option<ConnectionId> {
        self.active
            .and_then(|a| self.route.entries.get(a))
            .map(|e| e.connection.clone())
    }

    /// Make entry `to` the active one. A fresh session's first choice is not a switch.
    fn move_to(&mut self, to: usize, connection: &ConnectionId, reason: SwitchReason, now: u64) {
        if let Some(from) = self.active_connection() {
            self.last_switch = Some(Switch {
                from,
                to: connection.clone(),
                reason,
                at: now,
            });
        }
        self.active = Some(to);
        self.generation += 1;
    }
}

#[derive(Debug)]
pub struct RouteState {
    sessions: BTreeMap<RouteId, Session>,
    availability: Availability,
    /// The last problem seen per connection, for the status.
    problems: BTreeMap<ConnectionId, Problem>,
    last_lease: u64,
    /// The availability revision last confirmed written.
    handed: u64,
}

/// The problem an observation notes against its connection.
fn problem_of(seen: &Observation) -> Option<ProblemKind> {
    match seen {
        Observation::Throttled { .. } => Some(ProblemKind::Throttled),
        Observation::Faulted { .. } => Some(ProblemKind::Faulted),
        Observation::Unauthorised => Some(ProblemKind::Unauthorised),
        Observation::Forbidden => Some(ProblemKind::Forbidden),
        Observation::Rejected => Some(ProblemKind::Rejected),
        Observation::NotSent => Some(ProblemKind::Unreachable),
        Observation::Dropped => Some(ProblemKind::Interrupted),
        Observation::Completed | Observation::Exhausted(_) | Observation::Cancelled => None,
    }
}

/// The wait before a retry of `attempt`, or None when its budget is spent (D12).
fn retry_wait(attempt: &Attempt, retry_after: Option<u64>, now: u64) -> Option<u64> {
    let wait_ms = retry_after.map_or(RETRY_WAIT_MS, |secs| secs.saturating_mul(1000));
    (attempt.retries < RETRIES_MAX
        && now.saturating_sub(attempt.started_at) < ATTEMPT_WINDOW_SECS
        && wait_ms <= WAIT_MAX_MS)
        .then_some(wait_ms)
}

impl RouteState {
    #[must_use]
    pub fn new(records: Vec<ExhaustionRecord>) -> Self {
        let availability = Availability::load(records);
        Self {
            handed: availability.revision(),
            availability,
            sessions: BTreeMap::new(),
            problems: BTreeMap::new(),
            last_lease: 0,
        }
    }

    /// A desk was launched routed. Replaces a session of the same id.
    pub fn open(&mut self, id: RouteId, route: SessionRoute) {
        self.sessions.insert(
            id,
            Session {
                route,
                active: None,
                generation: 0,
                needs: Needs::default(),
                blocked_since: None,
                last_switch: None,
                last_served: None,
            },
        );
    }

    pub fn close(&mut self, id: &RouteId) {
        self.sessions.remove(id);
    }

    /// Admit a request: a lease to call upstream with, or why not. An error means no upstream call.
    pub fn begin(
        &mut self,
        id: &RouteId,
        request: Requirements,
        credentials: &BTreeSet<ConnectionId>,
        now: u64,
    ) -> Result<Lease, RouteError> {
        let attempt = Attempt {
            visited: Vec::new(),
            retries: 0,
            started_at: now,
        };
        self.issue(id, request, attempt, credentials, now, true)
    }

    /// Plan and cut one lease. `may_move` false: a plan that would move the session is refused,
    /// with nothing changed (the compare-and-swap of `settle`, and every retry).
    fn issue(
        &mut self,
        id: &RouteId,
        request: Requirements,
        attempt: Attempt,
        credentials: &BTreeSet<ConnectionId>,
        now: u64,
        may_move: bool,
    ) -> Result<Lease, RouteError> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or(RouteError::UnknownSession)?;
        let view = SessionView {
            active: session.active,
            needs: &session.needs,
        };
        let planned = plan_attempt(
            &session.route,
            &view,
            &self.availability,
            &request,
            &attempt,
            credentials,
            now,
        );
        let plan = match planned {
            Ok(plan) => plan,
            Err(error) => {
                if matches!(error, RouteError::Blocked { .. }) {
                    session.blocked_since.get_or_insert(now);
                }
                return Err(error);
            }
        };
        if plan.moves && !may_move {
            return Err(RouteError::Blocked {
                reasons: Vec::new(),
            });
        }
        let (connection, kind) = (plan.connection, plan.kind);
        if plan.moves {
            session.move_to(plan.entry, &connection, SwitchReason::Exhausted, now);
        }
        session.blocked_since = None;
        let class = session.route.class_of(&request.model);
        session.needs.note(&class, &request);
        let (model, effort) = match &plan.target {
            Target::AsSent => (None, request.effort.clone()),
            Target::Rewrite { model, effort } => (Some(model.clone()), effort.clone()),
        };
        session.last_served = Some(Served {
            connection: connection.clone(),
            requested_model: request.model.clone(),
            requested_effort: request.effort.clone(),
            model,
            effort,
            at: now,
        });
        self.last_lease += 1;
        self.availability.take(&plan.tickets, self.last_lease, now);
        Ok(Lease {
            id: self.last_lease,
            session: id.clone(),
            generation: session.generation,
            entry: plan.entry,
            connection,
            kind,
            target: plan.target,
            tickets: plan.tickets,
            request,
            attempt,
            class,
        })
    }

    /// Report what became of a lease. `relayed`: at least one byte of the model's reply has gone to the client.
    pub fn settle(
        &mut self,
        lease: Lease,
        seen: &Observation,
        relayed: bool,
        credentials: &BTreeSet<ConnectionId>,
        now: u64,
    ) -> Verdict {
        let Lease {
            id: lease_id,
            session: id,
            generation,
            entry,
            connection,
            tickets,
            request,
            mut attempt,
            class,
            ..
        } = lease;
        // The evidence is about the connection, not the session: it is kept either way.
        if let Observation::Exhausted(seen) = seen {
            let bucket = match seen.breadth {
                Breadth::Whole => Bucket::Whole,
                Breadth::Request => class,
            };
            self.availability.mark(&connection, bucket, seen, now);
        }
        let Some(current) = self.sessions.get(&id).map(|s| s.generation) else {
            self.availability.give_back(&tickets, lease_id);
            return Verdict::Done;
        };
        match seen {
            Observation::Completed => {
                self.availability.clear(&tickets, lease_id);
                self.problems.remove(&connection);
            }
            Observation::Exhausted(_) => self.availability.spend(&tickets, lease_id),
            _ => self.availability.give_back(&tickets, lease_id),
        }
        if let Some(kind) = problem_of(seen) {
            self.problems.insert(connection, Problem { kind, at: now });
        }
        if relayed {
            return match seen {
                Observation::Completed | Observation::Cancelled => Verdict::Done,
                _ => Verdict::Cut,
            };
        }
        let (retry_after, fallback) = match seen {
            Observation::Completed | Observation::Cancelled => return Verdict::Done,
            Observation::Exhausted(_) => {
                attempt.visited.push(entry);
                let may_move = generation == current;
                return self
                    .issue(&id, request, attempt, credentials, now, may_move)
                    .map_or(Verdict::Relay, |lease| Verdict::Again { lease, wait_ms: 0 });
            }
            Observation::Unauthorised | Observation::Forbidden | Observation::Rejected => {
                return Verdict::Relay;
            }
            Observation::Dropped => return Verdict::Local(LocalFault::Interrupted),
            Observation::Throttled { retry_after } | Observation::Faulted { retry_after } => {
                (*retry_after, Verdict::Relay)
            }
            Observation::NotSent => (None, Verdict::Local(LocalFault::Unreachable)),
        };
        let Some(wait_ms) = retry_wait(&attempt, retry_after, now) else {
            return fallback;
        };
        attempt.retries += 1;
        match self.issue(&id, request, attempt, credentials, now, false) {
            Ok(lease) => Verdict::Again { lease, wait_ms },
            Err(_) => fallback,
        }
    }

    /// A Human asks for one check of an exhausted connection. False: it has no record.
    pub fn retry(&mut self, connection: &ConnectionId, now: u64) -> bool {
        self.availability.arm(connection, now)
    }

    /// A Human moves a session to another entry of its own chain.
    pub fn switch(
        &mut self,
        id: &RouteId,
        connection: &ConnectionId,
        credentials: &BTreeSet<ConnectionId>,
        now: u64,
    ) -> Result<(), RouteError> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or(RouteError::UnknownSession)?;
        let (to, entry) = session
            .route
            .entries
            .iter()
            .enumerate()
            .find(|(_, e)| e.connection == *connection)
            .ok_or(RouteError::NotInChain)?;
        standing(entry, credentials).map_err(|reason| RouteError::Refused {
            connection: connection.clone(),
            reason,
        })?;
        self.availability.arm(connection, now);
        if session.active != Some(to) {
            session.move_to(to, connection, SwitchReason::Manual, now);
        }
        session.blocked_since = None;
        Ok(())
    }

    /// Settings changed: narrow every open session and drop the records of connections that are gone.
    pub fn apply_preferences(&mut self, settings: &RoutingSettings) {
        for session in self.sessions.values_mut() {
            session.route.narrow(settings);
        }
        let known: BTreeSet<ConnectionId> =
            settings.connections.iter().map(|c| c.id.clone()).collect();
        self.availability.keep_only(&known);
        self.problems.retain(|c, _| known.contains(c));
    }

    #[must_use]
    pub fn status(
        &self,
        id: &RouteId,
        credentials: &BTreeSet<ConnectionId>,
        now: u64,
    ) -> Option<RouteStatus> {
        let session = self.sessions.get(id)?;
        let needs = &session.needs;
        let entries = session
            .route
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let records = self.availability.of(&entry.connection, now);
                let shut = records.iter().any(|(r, _, _)| match &r.bucket {
                    Bucket::Whole => true,
                    Bucket::Tier { tier } => needs.tiers.contains(tier),
                    Bucket::Model { model } => needs.unmapped.contains(model),
                });
                let state = if session.active == Some(i) {
                    EntryState::Active
                } else if let Err(reason) =
                    standing(entry, credentials).and_then(|()| fits_session(entry, needs))
                {
                    EntryState::Unavailable { reason }
                } else if shut {
                    EntryState::Unavailable {
                        reason: Skip::Exhausted,
                    }
                } else {
                    EntryState::Ready
                };
                EntryStatus {
                    connection: entry.connection.clone(),
                    name: entry.name.clone(),
                    kind: entry.kind,
                    state,
                    exhausted: records
                        .into_iter()
                        .map(|(record, checking, retry_armed)| ExhaustedScope {
                            bucket: record.bucket,
                            window: record.window,
                            observed_at: record.observed_at,
                            reset_at: record.reset_at,
                            checking,
                            retry_armed,
                        })
                        .collect(),
                    problem: self.problems.get(&entry.connection).copied(),
                }
            })
            .collect();
        Some(RouteStatus {
            context: session.route.context.clone(),
            generation: session.generation,
            active: session.active_connection(),
            blocked_since: session.blocked_since,
            last_switch: session.last_switch.clone(),
            last_served: session.last_served.clone(),
            entries,
        })
    }

    /// The records to write, when they changed since the last `saved`. Offered again until then,
    /// so a write that failed is tried again.
    #[must_use]
    pub fn unsaved(&self) -> Option<Saved> {
        let revision = self.availability.revision();
        (revision > self.handed).then(|| Saved {
            revision,
            records: self.availability.records(),
        })
    }

    /// The write of `revision` went through.
    pub fn saved(&mut self, revision: u64) {
        self.handed = self.handed.max(revision);
    }
}

#[cfg(test)]
mod tests;
