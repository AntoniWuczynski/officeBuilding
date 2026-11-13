//! The state machine that compares each desk's state between scans and decides which alerts
//! are due. No I/O and no clock of its own: time comes in as arguments.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use super::{Alert, AlertKind, DeskState, Look};
use crate::model::SessionState;

/// A sampled state must hold this long before it counts.
pub const SAMPLED_HOLD: Duration = Duration::from_secs(6);

/// A state that differs from the settled one and has not held long enough yet.
struct Pending {
    state: SessionState,
    since: Instant,
    hold: Duration,
}

struct Track {
    /// The state alerts were last judged against.
    settled: SessionState,
    /// The last scan had this desk's state reported as it happens.
    hooked: bool,
    pending: Option<Pending>,
}

#[derive(Default)]
pub struct Tracker {
    desks: BTreeMap<String, Track>,
    queue: Vec<Alert>,
    last_delivery: Option<Instant>,
}

/// The alert a settled change from `from` to `to` raises, if any.
fn classify(from: SessionState, to: SessionState) -> Option<AlertKind> {
    match to {
        SessionState::WaitingHuman => Some(AlertKind::NeedsYou),
        SessionState::Error => Some(AlertKind::Stuck),
        SessionState::Idle if matches!(from, SessionState::Running | SessionState::Thinking) => {
            Some(AlertKind::Finished)
        }
        SessionState::Idle
        | SessionState::Running
        | SessionState::Thinking
        | SessionState::Done => None,
    }
}

/// The settled state an alert of this kind is about.
fn target(kind: AlertKind) -> SessionState {
    match kind {
        AlertKind::NeedsYou => SessionState::WaitingHuman,
        AlertKind::Stuck => SessionState::Error,
        AlertKind::Finished => SessionState::Idle,
    }
}

/// The event's switch is on, and the human is not looking at that desk right now.
fn wanted(kind: AlertKind, desk_id: &str, look: &Look<'_>) -> bool {
    let switch = match kind {
        AlertKind::NeedsYou => look.settings.needs_you,
        AlertKind::Stuck => look.settings.stuck,
        AlertKind::Finished => look.settings.finished,
    };
    switch && !(look.focused && look.open_desk == Some(desk_id))
}

impl Tracker {
    /// Take in one scan. Returns the alerts to deliver now as one batch (empty: nothing is due).
    pub fn observe(&mut self, desks: &[DeskState], look: &Look<'_>, now: Instant) -> Vec<Alert> {
        let present: BTreeSet<&str> = desks.iter().map(|d| d.id.as_str()).collect();
        self.desks.retain(|id, _| present.contains(id.as_str()));
        self.queue.retain(|a| present.contains(a.desk_id.as_str()));

        for desk in desks {
            let Some(track) = self.desks.get_mut(&desk.id) else {
                self.desks.insert(
                    desk.id.clone(),
                    Track {
                        settled: desk.state,
                        hooked: desk.hooked,
                        pending: None,
                    },
                );
                continue;
            };
            // A feed event expiring hands the state back to sampling. That is no change of
            // the desk, so adopt it without an alert.
            let lost_feed = track.hooked && !desk.hooked;
            track.hooked = desk.hooked;
            if lost_feed && desk.state != track.settled {
                track.settled = desk.state;
                track.pending = None;
                continue;
            }
            if desk.state == track.settled {
                track.pending = None;
                continue;
            }
            let hold = if desk.hooked {
                Duration::ZERO
            } else {
                SAMPLED_HOLD
            };
            let since = match &track.pending {
                Some(p) if p.state == desk.state => p.since,
                _ => now,
            };
            if now.saturating_duration_since(since) < hold {
                track.pending = Some(Pending {
                    state: desk.state,
                    since,
                    hold,
                });
                continue;
            }
            let from = track.settled;
            track.settled = desk.state;
            track.pending = None;
            if let Some(kind) = classify(from, desk.state)
                && wanted(kind, &desk.id, look)
            {
                self.queue.push(Alert {
                    kind,
                    desk_id: desk.id.clone(),
                    title: desk.title.clone(),
                    floor: desk.floor.clone(),
                });
            }
        }

        self.flush(look, now)
    }

    fn flush(&mut self, look: &Look<'_>, now: Instant) -> Vec<Alert> {
        let cooldown = Duration::from_secs(u64::from(look.settings.cooldown_secs));
        let ready = self
            .last_delivery
            .is_none_or(|last| now.saturating_duration_since(last) >= cooldown);
        if self.queue.is_empty() || !ready {
            return Vec::new();
        }
        let mut last_per_desk: BTreeMap<String, Alert> = BTreeMap::new();
        for alert in self.queue.drain(..) {
            let still_due = self
                .desks
                .get(&alert.desk_id)
                .is_some_and(|t| t.settled == target(alert.kind));
            if still_due && wanted(alert.kind, &alert.desk_id, look) {
                last_per_desk.insert(alert.desk_id.clone(), alert);
            }
        }
        let mut batch: Vec<Alert> = last_per_desk.into_values().collect();
        batch.sort_by(|a, b| (a.kind, &a.title, &a.desk_id).cmp(&(b.kind, &b.title, &b.desk_id)));
        if !batch.is_empty() {
            self.last_delivery = Some(now);
        }
        batch
    }

    /// How long until the tracker next has something to do without a state change:
    /// a held state maturing, or a queued alert's cooldown ending. None: nothing is waiting.
    #[must_use]
    pub fn next_wake(&self, now: Instant, cooldown: Duration) -> Option<Duration> {
        let held = self
            .desks
            .values()
            .filter_map(|t| t.pending.as_ref())
            .map(|p| p.since.checked_add(p.hold).unwrap_or(now));
        let queued = (!self.queue.is_empty()).then(|| {
            self.last_delivery
                .map_or(now, |last| last.checked_add(cooldown).unwrap_or(now))
        });
        held.chain(queued)
            .min()
            .map(|at| at.saturating_duration_since(now))
    }
}
