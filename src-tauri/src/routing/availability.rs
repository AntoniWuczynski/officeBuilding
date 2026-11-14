//! Scoped exhaustion records and the one-probe recovery rule. Pure: time is passed in.
//!
//! A record is cleared only by a successful check of that same record (D7). A reset time in
//! the past opens nothing by itself: it allows exactly one check, and the same reset time
//! reported again allows no further one (D8).

use super::config::{ConnectionId, TierId};
use super::observe::Exhaustion;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The part of a connection's allowance a record covers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Bucket {
    Whole,
    Tier {
        tier: TierId,
    },
    /// A model no tier lists.
    Model {
        model: String,
    },
}

/// One saved observation that an allowance is used up. No content, no credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExhaustionRecord {
    pub connection: ConnectionId,
    pub bucket: Bucket,
    /// When the refusal was seen (Unix seconds).
    pub observed_at: u64,
    #[serde(default)]
    pub reset_at: Option<u64>,
    #[serde(default)]
    pub window: Option<String>,
    /// When a check of this record was last let through.
    #[serde(default)]
    pub last_probe_at: Option<u64>,
}

/// The right to check one record with one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub connection: ConnectionId,
    pub bucket: Bucket,
    pub epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// No record covers this request.
    Open,
    /// Records cover it, and every one may be checked now: take these tickets.
    Probe(Vec<Ticket>),
    /// A record that covers it is being checked by another request.
    Probing,
    /// Records cover it and it may not be checked.
    Shut,
}

/// How long a check may run before it counts as given back.
pub const PROBE_LEASE_SECS: u64 = 900;
/// The longest window name kept from a provider.
pub const WINDOW_MAX: usize = 40;

/// A check in progress, with what the record was before it.
#[derive(Debug, Clone)]
struct Probing {
    lease: u64,
    since: u64,
    was_armed: bool,
    prev_last_probe_at: Option<u64>,
}

/// A record with its in-memory state. Only `record` is saved.
#[derive(Debug, Clone)]
struct Slot {
    record: ExhaustionRecord,
    epoch: u64,
    armed: bool,
    probing: Option<Probing>,
}

impl Slot {
    /// The check in progress, unless its lease ran out.
    fn live(&self, now: u64) -> Option<&Probing> {
        self.probing
            .as_ref()
            .filter(|p| p.since.saturating_add(PROBE_LEASE_SECS) > now)
    }

    /// `armed` and `last_probe_at` as they were before any check that is still recorded.
    /// Read only when no live check holds the record, so a stale check counts as given back.
    fn settled(&self) -> (bool, Option<u64>) {
        match &self.probing {
            Some(p) => (p.was_armed, p.prev_last_probe_at),
            None => (self.armed, self.record.last_probe_at),
        }
    }

    fn checkable(&self, now: u64) -> bool {
        let (armed, last_probe_at) = self.settled();
        armed || due(self.record.reset_at, last_probe_at, now)
    }

    fn probing_under(&self, lease: u64) -> bool {
        self.probing.as_ref().is_some_and(|p| p.lease == lease)
    }
}

/// Due for an automatic check: the reset time has come and no check was let through since (D8).
fn due(reset_at: Option<u64>, last_probe_at: Option<u64>, now: u64) -> bool {
    reset_at.is_some_and(|reset| now >= reset && last_probe_at.is_none_or(|last| last < reset))
}

/// A provider's window name, kept only when it is short plain text.
fn window(raw: Option<&str>) -> Option<String> {
    raw.filter(|w| {
        (1..=WINDOW_MAX).contains(&w.chars().count())
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    })
    .map(str::to_string)
}

#[derive(Debug, Default)]
pub struct Availability {
    slots: BTreeMap<ConnectionId, BTreeMap<Bucket, Slot>>,
    next_epoch: u64,
    revision: u64,
}

impl Availability {
    #[must_use]
    pub fn load(records: Vec<ExhaustionRecord>) -> Self {
        let mut availability = Self::default();
        for record in records {
            let epoch = availability.fresh_epoch();
            availability
                .slots
                .entry(record.connection.clone())
                .or_default()
                .insert(
                    record.bucket.clone(),
                    Slot {
                        record,
                        epoch,
                        armed: false,
                        probing: None,
                    },
                );
        }
        availability
    }

    /// Every record, for saving. Stable order: by connection, then bucket.
    /// A check in progress is not saved: it is written when it ends, so a quit mid-check leaves
    /// the record as it was and the next start can check again.
    #[must_use]
    pub fn records(&self) -> Vec<ExhaustionRecord> {
        self.slots
            .values()
            .flat_map(BTreeMap::values)
            .map(|slot| ExhaustionRecord {
                last_probe_at: slot.settled().1,
                ..slot.record.clone()
            })
            .collect()
    }

    /// Goes up by one on every change to what `records` returns.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn gate(&self, connection: &ConnectionId, class: &Bucket, now: u64) -> Gate {
        let covering: Vec<&Slot> = self
            .slots
            .get(connection)
            .into_iter()
            .flatten()
            .filter(|(bucket, _)| **bucket == Bucket::Whole || *bucket == class)
            .map(|(_, slot)| slot)
            .collect();
        if covering.is_empty() {
            Gate::Open
        } else if covering.iter().any(|slot| slot.live(now).is_some()) {
            Gate::Probing
        } else if covering.iter().all(|slot| slot.checkable(now)) {
            Gate::Probe(
                covering
                    .iter()
                    .map(|slot| Ticket {
                        connection: slot.record.connection.clone(),
                        bucket: slot.record.bucket.clone(),
                        epoch: slot.epoch,
                    })
                    .collect(),
            )
        } else {
            Gate::Shut
        }
    }

    /// Hand the tickets to lease `lease`.
    pub fn take(&mut self, tickets: &[Ticket], lease: u64, now: u64) {
        for ticket in tickets {
            let Some(slot) = self.slot_mut(ticket) else {
                continue;
            };
            if slot.epoch != ticket.epoch || slot.live(now).is_some() {
                continue;
            }
            let (was_armed, prev_last_probe_at) = slot.settled();
            slot.probing = Some(Probing {
                lease,
                since: now,
                was_armed,
                prev_last_probe_at,
            });
            slot.armed = false;
            slot.record.last_probe_at = Some(now);
        }
    }

    /// The check passed: remove each ticket's record, if it is still the record the ticket was cut from.
    pub fn clear(&mut self, tickets: &[Ticket], lease: u64) {
        let mut changed = false;
        for ticket in tickets {
            let Some(buckets) = self.slots.get_mut(&ticket.connection) else {
                continue;
            };
            let taken = buckets
                .get(&ticket.bucket)
                .is_some_and(|slot| slot.epoch == ticket.epoch && slot.probing_under(lease));
            if taken {
                buckets.remove(&ticket.bucket);
                changed = true;
                if buckets.is_empty() {
                    self.slots.remove(&ticket.connection);
                }
            }
        }
        self.bump_if(changed);
    }

    /// The check learned nothing: each record is as it was before `take`.
    pub fn give_back(&mut self, tickets: &[Ticket], lease: u64) {
        for ticket in tickets {
            let Some(slot) = self.slot_mut(ticket) else {
                continue;
            };
            if !slot.probing_under(lease) {
                continue;
            }
            let (armed, last_probe_at) = slot.settled();
            slot.armed = armed;
            slot.record.last_probe_at = last_probe_at;
            slot.probing = None;
        }
    }

    /// The check is over and is not to be repeated by itself: release without restoring.
    /// The check time is now saved.
    pub fn spend(&mut self, tickets: &[Ticket], lease: u64) {
        let mut changed = false;
        for ticket in tickets {
            if let Some(slot) = self.slot_mut(ticket)
                && slot.probing_under(lease)
            {
                slot.probing = None;
                changed = true;
            }
        }
        self.bump_if(changed);
    }

    /// A refusal was seen: add or renew the record for `bucket`.
    pub fn mark(&mut self, connection: &ConnectionId, bucket: Bucket, seen: &Exhaustion, now: u64) {
        let epoch = self.fresh_epoch();
        let buckets = self.slots.entry(connection.clone()).or_default();
        let last_probe_at = buckets
            .get(&bucket)
            .and_then(|slot| slot.record.last_probe_at);
        buckets.insert(
            bucket.clone(),
            Slot {
                record: ExhaustionRecord {
                    connection: connection.clone(),
                    bucket,
                    observed_at: now,
                    reset_at: seen.reset_at,
                    window: window(seen.window.as_deref()),
                    last_probe_at,
                },
                epoch,
                armed: false,
                probing: None,
            },
        );
        self.revision += 1;
    }

    /// A Human asked for a retry: every record of `connection` may be checked once. False: it has none.
    pub fn arm(&mut self, connection: &ConnectionId, now: u64) -> bool {
        let Some(buckets) = self.slots.get_mut(connection) else {
            return false;
        };
        for slot in buckets.values_mut().filter(|slot| slot.live(now).is_none()) {
            // A check whose lease ran out counts as given back.
            (slot.armed, slot.record.last_probe_at) = slot.settled();
            slot.probing = None;
            slot.armed = true;
        }
        true
    }

    /// Drop the records of connections that are not in `known`.
    pub fn keep_only(&mut self, known: &BTreeSet<ConnectionId>) {
        let before = self.slots.len();
        self.slots
            .retain(|connection, _| known.contains(connection));
        self.bump_if(self.slots.len() != before);
    }

    /// For the status: a connection's records, each with whether it is being checked and whether a retry is armed.
    #[must_use]
    pub fn of(&self, connection: &ConnectionId, now: u64) -> Vec<(ExhaustionRecord, bool, bool)> {
        self.slots
            .get(connection)
            .into_iter()
            .flat_map(BTreeMap::values)
            .map(|slot| (slot.record.clone(), slot.live(now).is_some(), slot.armed))
            .collect()
    }

    fn slot_mut(&mut self, ticket: &Ticket) -> Option<&mut Slot> {
        self.slots
            .get_mut(&ticket.connection)
            .and_then(|buckets| buckets.get_mut(&ticket.bucket))
    }

    fn fresh_epoch(&mut self) -> u64 {
        self.next_epoch += 1;
        self.next_epoch
    }

    fn bump_if(&mut self, changed: bool) {
        if changed {
            self.revision += 1;
        }
    }
}

#[cfg(test)]
mod tests;
