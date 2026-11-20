//! Moving a session to another floor (island), kept in the app's records while the
//! session stays on the building's books.

use super::{Inner, Office, claim_owned};
use crate::discovery::{self, Discovered};
use crate::focus;
use crate::store::Records;
use std::collections::BTreeMap;

/// Forget the moves whose session has been off the books too long, and note which are off it now.
/// A move never confirmed (no `move_seen` entry) goes at its first miss. True when anything changed.
pub(super) fn prune_moves(
    records: &mut Records,
    on_books: impl Fn(&str) -> bool,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let before = (records.moved_sessions.len(), records.move_seen.clone());
    let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let Records {
        moved_sessions,
        move_seen,
        ..
    } = records;
    moved_sessions.retain(|id, _| {
        if on_books(id) {
            move_seen.insert(id.clone(), None);
            return true;
        }
        match move_seen.get(id).cloned() {
            None => false,
            Some(None) => {
                move_seen.insert(id.clone(), Some(stamp.clone()));
                true
            }
            Some(Some(since)) => chrono::DateTime::parse_from_rfc3339(&since).map_or_else(
                |_| {
                    move_seen.insert(id.clone(), Some(stamp.clone()));
                    true
                },
                |t| {
                    (now - t.to_utc())
                        .to_std()
                        .is_ok_and(|gone| gone <= discovery::KEEP_FINISHED)
                },
            ),
        }
    });
    move_seen.retain(|id, _| moved_sessions.contains_key(id));
    before != (moved_sessions.len(), move_seen.clone())
}

/// Put each session the human moved on the floor they chose, wherever it signs out too, and
/// say where each one was found. A move to a floor that is not here is ignored.
pub(super) fn apply_moves(
    found: &mut Discovered,
    moves: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut from = BTreeMap::new();
    for (id, to) in moves {
        if !found.projects.iter().any(|p| p.id == *to) {
            continue;
        }
        let live = found
            .sessions
            .iter_mut()
            .find(|s| s.id == *id)
            .map(|s| std::mem::replace(&mut s.project_id, to.clone()));
        let signed_out = found
            .ended
            .iter_mut()
            .find(|e| e.id == *id)
            .map(|e| std::mem::replace(&mut e.project_id, to.clone()));
        let Some(was) = live.clone().or(signed_out) else {
            continue;
        };
        if was == *to {
            continue;
        }
        from.insert(id.clone(), was.clone());
        if live.is_none() {
            continue;
        }
        for p in &mut found.projects {
            if p.id == was {
                p.session_ids.retain(|s| s != id);
            } else if p.id == *to {
                p.session_ids.push(id.clone());
            }
        }
        for q in found.questions.iter_mut().filter(|q| q.session_id == *id) {
            q.project_id.clone_from(to);
        }
    }
    from
}

/// A session that takes over a terminal from another (after a `/clear`, or as a handover's
/// receiver) takes that one's move, once, when it first shows up there. True when one was taken.
pub(super) fn inherit_moves(
    records: &mut Records,
    before: &BTreeMap<String, String>,
    owned: &BTreeMap<String, String>,
    handed_from: &BTreeMap<String, String>,
) -> bool {
    let mut taken = false;
    for (terminal, id) in owned {
        let from = match before.get(terminal) {
            Some(was) if was == id => continue,
            Some(was) => was,
            None => match handed_from.get(terminal) {
                Some(from) => from,
                None => continue,
            },
        };
        if records.moved_sessions.contains_key(id) {
            continue;
        }
        let Some(to) = records.moved_sessions.get(from).cloned() else {
            continue;
        };
        records.moved_sessions.insert(id.clone(), to);
        records.move_seen.insert(id.clone(), None);
        taken = true;
    }
    taken
}

/// Bind the running sessions to the terminals that run them (`claim_owned`), and give a
/// session that took over a terminal the move of the one before it. Returns the sessions
/// running elsewhere.
pub(super) fn claim_terminals(
    inner: &mut Inner,
    running: &BTreeMap<i32, String>,
    found: &mut Discovered,
) -> Vec<String> {
    let before = inner.owned.clone();
    let discovery = &inner.discovery;
    let elsewhere = claim_owned(
        &mut found.sessions,
        &mut inner.owned,
        running,
        |id| discovery.pid_of(id),
        focus::parent_pid,
    );
    let handed_from = inner.handover.handed_from();
    if inherit_moves(
        &mut inner.store.records,
        &before,
        &inner.owned,
        &handed_from,
    ) {
        // A failed save only leaves the move to be saved with the next change.
        let _ = inner.store.save();
        inner
            .moved_from
            .extend(apply_moves(found, &inner.store.records.moved_sessions));
    }
    elsewhere
}

impl Office {
    /// Put a session on another floor, or back on the one it was found on (`None`, or that floor).
    /// The move outlives the session only as long as the session stays on the building's books.
    pub fn move_session(&self, session_id: &str, project_id: Option<&str>) -> Result<(), String> {
        {
            let mut inner = self.lock();
            // Its seat or reviewer's desk stays on its own floor.
            if Self::staff_sessions(&inner).iter().any(|s| s == session_id) {
                return Err("a floor's manager or QA reviewer is not moved".to_string());
            }
            let sits = |p: &str| inner.found.projects.iter().any(|x| x.id == p);
            let now = inner
                .found
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .map(|s| &s.project_id)
                .or_else(|| {
                    inner
                        .found
                        .ended
                        .iter()
                        .find(|e| e.id == session_id)
                        .map(|e| &e.project_id)
                })
                .ok_or_else(|| format!("{session_id} is not a session of this building"))?;
            let home = inner.moved_from.get(session_id).unwrap_or(now);
            let target = match project_id {
                Some(to) if to != home => {
                    if to == self.setup_project_id {
                        return Err("the Setup floor holds the library, not agents".to_string());
                    }
                    if !sits(to) {
                        return Err(format!("{to} is not a floor of this building"));
                    }
                    Some(to.to_string())
                }
                _ => None,
            };
            let home_sunk = !sits(home);
            let moves = &mut inner.store.records.moved_sessions;
            if target.is_none() && home_sunk && moves.contains_key(session_id) {
                return Err(
                    "the island it came from was sunk, so the agent has nowhere to go back to"
                        .to_string(),
                );
            }
            match target {
                Some(to) => {
                    moves.insert(session_id.to_string(), to);
                    inner
                        .store
                        .records
                        .move_seen
                        .insert(session_id.to_string(), None);
                }
                None => {
                    moves.remove(session_id);
                }
            }
            inner.store.save()?;
        }
        self.refresh();
        Ok(())
    }
}
