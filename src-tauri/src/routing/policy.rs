//! The selection policy: which entry of a session's chain one request may use, and why the
//! others may not. Pure: it changes nothing and reads no clock.

use super::availability::{Availability, Bucket, Gate, Ticket};
use super::capabilities::{Capability, Requirements, supports};
use super::config::{ConnectionId, ConnectionKind, ContextId, Targets, TierId, TierTarget, Tiers};
use crate::settings::RoutingSettings;
use serde::Serialize;
use std::collections::BTreeSet;

/// A session's own copy of its context's chain, taken when it is opened (D10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRoute {
    pub context: ContextId,
    pub entries: Vec<Entry>,
    pub tiers: Tiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub connection: ConnectionId,
    pub name: String,
    pub kind: ConnectionKind,
    pub paid: bool,
    pub experimental: bool,
    /// It has left Settings since the session opened.
    pub removed: bool,
    pub targets: Targets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// The master switch is off.
    Off,
    UnknownContext,
    EmptyChain,
}

impl SessionRoute {
    /// Copy `context`'s chain out of the settings. A caller that gets an error launches the desk unrouted.
    pub fn open(settings: &RoutingSettings, context: &ContextId) -> Result<Self, OpenError> {
        if !settings.enabled {
            return Err(OpenError::Off);
        }
        let chain = &settings
            .context(context)
            .ok_or(OpenError::UnknownContext)?
            .chain;
        let mut seen = BTreeSet::new();
        let entries: Vec<Entry> = chain
            .iter()
            .filter(|id| seen.insert(*id))
            .filter_map(|id| settings.connection(id))
            .filter(|c| c.context == *context)
            .map(|c| Entry {
                connection: c.id.clone(),
                name: c.name.clone(),
                kind: c.kind,
                paid: c.paid,
                experimental: c.experimental,
                removed: false,
                targets: c.targets.clone(),
            })
            .collect();
        if entries.is_empty() {
            return Err(OpenError::EmptyChain);
        }
        Ok(Self {
            context: context.clone(),
            entries,
            tiers: settings.tiers.clone(),
        })
    }

    /// Take away what Settings no longer allow. Never gives anything back.
    pub fn narrow(&mut self, settings: &RoutingSettings) {
        for entry in &mut self.entries {
            match settings.connection(&entry.connection) {
                Some(now) if now.kind == entry.kind && now.context == self.context => {
                    entry.paid &= now.paid;
                    entry.experimental &= now.experimental;
                }
                _ => entry.removed = true,
            }
        }
    }

    /// The bucket a request for `model` falls in: its tier, or the model itself when no tier lists it.
    #[must_use]
    pub fn class_of(&self, model: &str) -> Bucket {
        match self.tiers.tier_of(model) {
            Some(tier) => Bucket::Tier { tier },
            None => Bucket::Model {
                model: model.to_string(),
            },
        }
    }
}

/// Why an entry may not be tried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Skip {
    /// It is no longer in Settings.
    Removed,
    /// It costs money and A Human has not allowed that.
    PaidNotAllowed,
    /// It is an experimental route and A Human has not allowed that.
    ExperimentalNotAllowed,
    /// No key is saved for it, or it needs a sign-in that does not exist yet.
    NoCredential,
    /// The model the desk asked for is in no tier.
    UnmappedModel { model: String },
    /// This connection has no model for that tier.
    NoTarget { tier: TierId },
    /// This connection has no mapping for that effort.
    EffortUnmapped { tier: TierId, effort: String },
    /// This connection cannot carry something the request needs.
    Unsupported { capability: Capability },
    /// Its allowance is used up.
    Exhausted,
    /// This request already tried it.
    Tried,
}

/// What an attempt sends upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Untouched: the model, the effort and the body the client sent.
    AsSent,
    /// The model replaced, and the effort replaced (None: the request carried none, so none is sent).
    Rewrite {
        model: String,
        effort: Option<String>,
    },
}

/// What a session has asked for so far. A connection the session moves to must serve all of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Needs {
    pub tiers: BTreeSet<TierId>,
    pub efforts: BTreeSet<(TierId, String)>,
    /// Models it asked for that are in no tier.
    pub unmapped: BTreeSet<String>,
    pub capabilities: BTreeSet<Capability>,
}

impl Needs {
    pub fn note(&mut self, class: &Bucket, request: &Requirements) {
        match class {
            Bucket::Tier { tier } => {
                self.tiers.insert(*tier);
                if let Some(effort) = &request.effort {
                    self.efforts.insert((*tier, effort.clone()));
                }
            }
            Bucket::Model { model } => {
                self.unmapped.insert(model.clone());
            }
            Bucket::Whole => {}
        }
        self.capabilities
            .extend(request.capabilities.iter().copied());
    }
}

/// The part of a session that selection reads.
#[derive(Debug, Clone, Copy)]
pub struct SessionView<'a> {
    pub active: Option<usize>,
    pub needs: &'a Needs,
}

/// One request's progress through the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    /// Entries this request has given up on, by index.
    pub visited: Vec<usize>,
    /// Times it has asked the same entry again after a transient failure.
    pub retries: u8,
    /// When its first attempt was planned (Unix seconds).
    pub started_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptPlan {
    pub entry: usize,
    /// That entry's connection and kind.
    pub connection: ConnectionId,
    pub kind: ConnectionKind,
    pub target: Target,
    /// The exhaustion records this attempt checks. Empty: an ordinary attempt.
    pub tickets: Vec<Ticket>,
    /// The session's active connection becomes this entry.
    pub moves: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// No such session (closed, or the app restarted).
    UnknownSession,
    /// The session's connection is healthy but cannot serve this request. Nothing switches.
    Refused {
        connection: ConnectionId,
        reason: Skip,
    },
    /// Nothing may be tried. One reason per entry, in chain order. No upstream is called.
    Blocked { reasons: Vec<(ConnectionId, Skip)> },
    /// `switch` named a connection that is not in the session's chain.
    NotInChain,
}

/// Whether an entry may be tried at all: still in Settings, allowed to spend money, allowed
/// if experimental, and holding a credential.
pub fn standing(entry: &Entry, credentials: &BTreeSet<ConnectionId>) -> Result<(), Skip> {
    if entry.removed {
        Err(Skip::Removed)
    } else if entry.kind.metered() && !entry.paid {
        Err(Skip::PaidNotAllowed)
    } else if entry.kind.experimental() && !entry.experimental {
        Err(Skip::ExperimentalNotAllowed)
    } else if entry.kind.needs_credential() && !credentials.contains(&entry.connection) {
        Err(Skip::NoCredential)
    } else {
        Ok(())
    }
}

/// The model this entry sends for `tier`. Never another tier's model.
fn target_for(entry: &Entry, tier: TierId) -> Result<&TierTarget, Skip> {
    entry
        .targets
        .get(tier)
        .as_ref()
        .ok_or(Skip::NoTarget { tier })
}

/// The effort sent for `effort` (D11): its pair, else the same name on a Messages route.
fn effort_for(
    kind: ConnectionKind,
    target: &TierTarget,
    tier: TierId,
    effort: &str,
) -> Result<String, Skip> {
    match target.efforts.get(effort) {
        Some(mapped) => Ok(mapped.clone()),
        None if !kind.translated() => Ok(effort.to_string()),
        None => Err(Skip::EffortUnmapped {
            tier,
            effort: effort.to_string(),
        }),
    }
}

fn carries<'a>(
    kind: ConnectionKind,
    mut capabilities: impl Iterator<Item = &'a Capability>,
) -> Result<(), Skip> {
    capabilities
        .find(|c| !supports(kind, **c))
        .map_or(Ok(()), |c| Err(Skip::Unsupported { capability: *c }))
}

/// What this entry would send for `request`, or the first reason it may not.
pub fn fit(
    entry: &Entry,
    class: &Bucket,
    request: &Requirements,
    credentials: &BTreeSet<ConnectionId>,
) -> Result<Target, Skip> {
    standing(entry, credentials)?;
    let target = if entry.kind.passes_through() {
        Target::AsSent
    } else {
        let Bucket::Tier { tier } = *class else {
            return Err(Skip::UnmappedModel {
                model: request.model.clone(),
            });
        };
        let target = target_for(entry, tier)?;
        let effort = match &request.effort {
            Some(effort) => Some(effort_for(entry.kind, target, tier, effort)?),
            None => None,
        };
        Target::Rewrite {
            model: target.model.clone(),
            effort,
        }
    };
    carries(entry.kind, request.capabilities.iter())?;
    Ok(target)
}

/// Whether this entry serves everything the session has asked for so far, so a fallback
/// chosen for one request does not strand the rest of the session.
pub fn fits_session(entry: &Entry, needs: &Needs) -> Result<(), Skip> {
    if entry.kind.passes_through() {
        return Ok(());
    }
    if let Some(model) = needs.unmapped.first() {
        return Err(Skip::UnmappedModel {
            model: model.clone(),
        });
    }
    for tier in &needs.tiers {
        target_for(entry, *tier)?;
    }
    for (tier, effort) in &needs.efforts {
        effort_for(entry.kind, target_for(entry, *tier)?, *tier, effort)?;
    }
    carries(entry.kind, needs.capabilities.iter())
}

/// Plan one attempt of `request` for a session. Pure: it changes nothing and reads no clock.
/// `credentials` holds the connections the building has a credential for now.
pub fn plan_attempt(
    route: &SessionRoute,
    session: &SessionView<'_>,
    availability: &Availability,
    request: &Requirements,
    attempt: &Attempt,
    credentials: &BTreeSet<ConnectionId>,
    now: u64,
) -> Result<AttemptPlan, RouteError> {
    let class = route.class_of(&request.model);
    // An open entry is planned as is. An exhausted one only as a probe, holding its tickets.
    let plan = |index: usize, entry: &Entry, target: Target, gate: Gate, moves: bool| {
        let tickets = match gate {
            Gate::Open => Vec::new(),
            Gate::Probe(tickets) => tickets,
            Gate::Probing | Gate::Shut => return None,
        };
        Some(AttemptPlan {
            entry: index,
            connection: entry.connection.clone(),
            kind: entry.kind,
            target,
            tickets,
            moves,
        })
    };

    // The active entry's reason, once it has one: it is reported in its place in the chain.
    let mut active: Option<(usize, Skip)> = None;
    let mut detour = false;
    if let Some((a, entry)) = session
        .active
        .and_then(|a| route.entries.get(a).map(|e| (a, e)))
    {
        if attempt.visited.contains(&a) {
            active = Some((a, Skip::Tried));
        } else {
            let target =
                fit(entry, &class, request, credentials).map_err(|reason| RouteError::Refused {
                    connection: entry.connection.clone(),
                    reason,
                })?;
            let gate = availability.gate(&entry.connection, &class, now);
            detour = gate == Gate::Probing;
            if let Some(found) = plan(a, entry, target, gate, false) {
                return Ok(found);
            }
            active = Some((a, Skip::Exhausted));
        }
    }

    let mut reasons = Vec::with_capacity(route.entries.len());
    for (i, entry) in route.entries.iter().enumerate() {
        let reason = if let Some((_, skip)) = active.as_ref().filter(|(a, _)| *a == i) {
            skip.clone()
        } else if attempt.visited.contains(&i) {
            Skip::Tried
        } else {
            let fitted = fit(entry, &class, request, credentials).and_then(|target| {
                if detour {
                    Ok(target)
                } else {
                    fits_session(entry, session.needs).map(|()| target)
                }
            });
            match fitted {
                Err(skip) => skip,
                Ok(target) => {
                    let gate = availability.gate(&entry.connection, &class, now);
                    match plan(i, entry, target, gate, !detour) {
                        Some(found) => return Ok(found),
                        None => Skip::Exhausted,
                    }
                }
            }
        };
        reasons.push((entry.connection.clone(), reason));
    }
    Err(RouteError::Blocked { reasons })
}

#[cfg(test)]
mod tests;
