//! What a Codex thread's own rollout says about its allowance and its goal
//! (handover H2 spec, sections 5.2 to 5.5). The proof is Codex's own record of
//! a used-up allowance: a `task_complete` whose error is `usage_limit_exceeded`,
//! seen when a local listener replayed a refusal to codex-cli 0.161.0. The
//! rollout format is not a stable interface
//! (<https://learn.chatgpt.com/docs/hooks>), so every line is untrusted, every
//! field is read leniently and anything unexpected proves nothing. Only the
//! few small fields the reducer looks at are declared: the rest, the account
//! identifiers in `session_meta` among them, is skipped unread.

use super::session_uuid;
use crate::discovery::claude_code::chat::{Loose, lenient};
use crate::discovery::claude_code::follow::{Fed, Follow};
use crate::discovery::collect_jsonl;
use crate::handover::record::RESET_HORIZON;
use crate::handover::work::Goal;
use crate::handover::{Allowance, Window};
use serde::de::value::MapAccessDeserializer;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

mod talk;
pub use talk::{Talk, conversation};

/// The id of Codex's built-in provider (V7).
pub const BUILT_IN_PROVIDER: &str = "openai";

/// The rollout of the thread `session_id` under `<home>/.codex/sessions`: a regular file (not
/// a link) named `rollout-…-<session_id>.jsonl` whose canonical path stays inside that
/// folder. When several match, the one modified last.
#[must_use]
pub fn find(home: &Path, session_id: &str) -> Option<PathBuf> {
    if session_uuid(session_id).as_deref() != Some(session_id) {
        return None;
    }
    let sessions = home.join(".codex").join("sessions");
    let root = fs::canonicalize(&sessions).ok()?;
    let mut files = Vec::new();
    collect_jsonl(&sessions, &mut files);
    files
        .into_iter()
        .filter(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| {
                    stem.starts_with("rollout-")
                        && session_uuid(stem).as_deref() == Some(session_id)
                })
        })
        .filter_map(|path| {
            let meta = fs::symlink_metadata(&path).ok()?;
            let inside = fs::canonicalize(&path).ok()?.starts_with(&root);
            (meta.is_file() && inside).then_some((meta.modified().ok()?, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// `T` read from a JSON object only. A derived struct also reads from an array, and an
/// array of the right words in the right order must never pass for a record.
#[derive(Default)]
struct Obj<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Obj<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Only<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Only<T> {
            type Value = T;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
                T::deserialize(MapAccessDeserializer::new(map))
            }
        }
        d.deserialize_map(Only(PhantomData)).map(Self)
    }
}

/// One rollout line, of any type.
#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    kind: String,
    /// RFC 3339.
    #[serde(default, deserialize_with = "lenient")]
    timestamp: Option<String>,
    #[serde(default)]
    payload: Obj<Payload>,
}

#[derive(Deserialize, Default)]
struct Payload {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    kind: String,
    // session_meta
    #[serde(default, deserialize_with = "lenient")]
    model_provider: Option<String>,
    /// Absent, null or a plain word for a top-level session, an object otherwise.
    #[serde(default)]
    source: Option<Loose<String>>,
    /// Present (with any value but null) only on a thread another spawned.
    #[serde(default)]
    parent_thread_id: Option<IgnoredAny>,
    /// Present (with any value but null) only on a fork, whose rollout copies its parent's lines.
    #[serde(default)]
    forked_from_id: Option<IgnoredAny>,
    // task_complete: absent or null is a clean turn, anything else is an error
    #[serde(default)]
    error: Option<Loose<Obj<TurnError>>>,
    #[serde(default, deserialize_with = "lenient")]
    completed_at: Option<i64>,
    // token_count
    #[serde(default, deserialize_with = "lenient")]
    rate_limits: Option<Obj<Limits>>,
    // thread_goal_updated
    #[serde(default)]
    goal: Option<Loose<Obj<GoalSeen>>>,
}

/// The error of a failed turn. Its `message` is never read: the sentence proves nothing.
#[derive(Deserialize, Default)]
struct TurnError {
    #[serde(default, deserialize_with = "lenient")]
    codex_error_info: Option<String>,
}

/// A `token_count`'s rate-limit snapshot.
#[derive(Deserialize, Default)]
struct Limits {
    #[serde(default, deserialize_with = "lenient")]
    limit_name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    primary: Option<Obj<WindowSeen>>,
    #[serde(default, deserialize_with = "lenient")]
    secondary: Option<Obj<WindowSeen>>,
}

#[derive(Deserialize, Default)]
struct WindowSeen {
    #[serde(default, deserialize_with = "lenient")]
    used_percent: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    window_minutes: Option<i64>,
    /// Unix seconds.
    #[serde(default, deserialize_with = "lenient")]
    resets_at: Option<i64>,
}

#[derive(Deserialize, Default)]
struct GoalSeen {
    #[serde(default, deserialize_with = "lenient")]
    status: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    objective: Option<String>,
}

/// What stands at the end of a thread's rollout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Reading {
    /// No failed turn, or a turn has completed cleanly since.
    #[default]
    Clear,
    /// Codex's own record that its allowance is used up.
    UsedUp {
        window: Option<Window>,
        resets_at: Option<i64>,
        /// When Codex wrote it, Unix seconds, if the record says.
        at: Option<i64>,
    },
    /// A turn's end that proves nothing about the allowance.
    Other(Why),
}

/// What a turn's end that is not the proof was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    OtherError,
    /// A limit on one model, named in the snapshot: Codex itself says to switch model (K1).
    ModelLimit,
    Aborted,
    Unreadable,
}

/// The rollout reduced line by line (spec 5.4).
#[derive(Default)]
struct Reducer {
    /// None until the first line is read. Then whether that line was a `session_meta` of a
    /// top-level thread on the built-in provider (K5).
    counts: Option<bool>,
    reading: Reading,
    goal: Goal,
    /// The last rate-limit snapshot of the turn in flight.
    snapshot: Option<Limits>,
}

impl Reducer {
    fn line(&mut self, raw: &str) {
        if raw.trim().is_empty() {
            return;
        }
        let Ok(Obj(line)) = serde_json::from_str::<Obj<Line>>(raw) else {
            self.counts.get_or_insert(false);
            self.reading = Reading::Other(Why::Unreadable);
            self.goal = Goal::Unknown;
            return;
        };
        let Obj(payload) = line.payload;
        if line.kind == "session_meta" {
            self.counts.get_or_insert_with(|| {
                payload.model_provider.as_deref() == Some(BUILT_IN_PROVIDER)
                    && payload
                        .source
                        .as_ref()
                        .is_none_or(|Loose(plain)| plain.is_some())
                    && payload.parent_thread_id.is_none()
                    && payload.forked_from_id.is_none()
            });
            return;
        }
        self.counts.get_or_insert(false);
        if line.kind != "event_msg" {
            return;
        }
        match payload.kind.as_str() {
            "task_started" | "turn_started" => self.snapshot = None,
            "token_count" => {
                if let Some(Obj(limits)) = payload.rate_limits {
                    self.snapshot = Some(limits);
                }
            }
            "task_complete" | "turn_complete" => {
                let at = payload.completed_at.or_else(|| {
                    let stamp = line.timestamp?;
                    Some(
                        chrono::DateTime::parse_from_rfc3339(&stamp)
                            .ok()?
                            .timestamp(),
                    )
                });
                self.reading = self.ended(payload.error, at);
            }
            "turn_aborted" => self.reading = Reading::Other(Why::Aborted),
            "thread_goal_updated" => self.goal = goal(payload.goal),
            _ => {}
        }
    }

    /// The reading a turn's end leaves, given its `error` and its time.
    fn ended(&self, error: Option<Loose<Obj<TurnError>>>, at: Option<i64>) -> Reading {
        let Some(Loose(error)) = error else {
            return Reading::Clear;
        };
        let info = error.and_then(|Obj(e)| e.codex_error_info);
        if info.as_deref() != Some("usage_limit_exceeded") {
            return Reading::Other(Why::OtherError);
        }
        let named = self
            .snapshot
            .as_ref()
            .and_then(|s| s.limit_name.as_deref())
            .map(str::trim)
            .is_some_and(|name| {
                !name.is_empty()
                    && !name.eq_ignore_ascii_case("codex")
                    && !name.eq_ignore_ascii_case("gpt-reserve")
            });
        if named {
            return Reading::Other(Why::ModelLimit);
        }
        let full = at
            .zip(self.snapshot.as_ref())
            .into_iter()
            .flat_map(|(at, s)| {
                [&s.primary, &s.secondary]
                    .into_iter()
                    .flatten()
                    .filter_map(move |Obj(w)| {
                        let reset = w
                            .resets_at
                            .filter(|&r| at < r && r <= at.saturating_add(RESET_HORIZON))?;
                        w.used_percent
                            .is_some_and(|used| used >= 100.0)
                            .then_some((reset, w.window_minutes))
                    })
            })
            .max_by_key(|(reset, _)| *reset);
        Reading::UsedUp {
            window: full.and_then(|(_, minutes)| match minutes {
                Some(300) => Some(Window::FiveHour),
                Some(10080) => Some(Window::SevenDay),
                _ => None,
            }),
            resets_at: full.map(|(reset, _)| reset),
            at,
        }
    }
}

/// What a `thread_goal_updated` says about the goal.
fn goal(seen: Option<Loose<Obj<GoalSeen>>>) -> Goal {
    let Some(Obj(seen)) = seen.and_then(|Loose(g)| g) else {
        return Goal::Unknown;
    };
    match seen.status.as_deref() {
        None => Goal::Unknown,
        Some("complete" | "paused") => Goal::None,
        Some(_) => Goal::Standing {
            objective: seen.objective.unwrap_or_default(),
        },
    }
}

/// Follows one Codex thread's rollout from its first byte.
pub struct RolloutReader {
    home: PathBuf,
    session_id: String,
    path: Option<PathBuf>,
    follow: Follow,
    reducer: Reducer,
}

impl RolloutReader {
    /// A reader for `session_id`. It does not touch the disk yet.
    #[must_use]
    pub fn new(home: &Path, session_id: &str) -> Self {
        Self {
            home: home.to_path_buf(),
            session_id: session_id.to_string(),
            path: None,
            follow: Follow::default(),
            reducer: Reducer::default(),
        }
    }

    /// Read what was appended since the last call: the whole file the first time (K14).
    pub fn read(&mut self) {
        if self.path.is_none() {
            self.path = find(&self.home, &self.session_id);
        }
        let Some(path) = &self.path else {
            return;
        };
        let reducer = &mut self.reducer;
        let read = self.follow.read(path, None, |fed| match fed {
            Fed::Fresh { .. } => *reducer = Reducer::default(),
            Fed::Line { text, .. } => reducer.line(text),
        });
        if read.is_none() {
            self.path = None;
        }
    }

    /// Whether this is a top-level thread on Codex's built-in provider (K5). False until
    /// its `session_meta` has been read.
    #[must_use]
    pub fn counts(&self) -> bool {
        self.reducer.counts == Some(true)
    }

    /// What the thread's own record says about its allowance. `Unproven` while `counts()` is false.
    #[must_use]
    pub fn allowance(&self) -> Allowance {
        if !self.counts() {
            return Allowance::Unproven;
        }
        match &self.reducer.reading {
            Reading::Clear => Allowance::Clear,
            Reading::UsedUp {
                window,
                resets_at,
                at,
            } => Allowance::UsedUp {
                window: *window,
                reset_at: *resets_at,
                at: *at,
            },
            Reading::Other(_) => Allowance::Unproven,
        }
    }

    /// What the rollout says about the thread's goal.
    #[must_use]
    pub fn goal(&self) -> &Goal {
        &self.reducer.goal
    }
}

#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod tests;
