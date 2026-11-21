//! The hook status feed: agents the building launches get hooks that drop each
//! event into a folder as its own file (`<Event>.XXXXXX`, the hook's JSON from
//! standard input). One file per event means hooks running side by side never
//! interleave, and the hook needs only `sh`, `mktemp` and `cat`. A refresh
//! collects the files, deletes them, and lets the newest event of a session set
//! its desk's state when it is newer than what the session's own files say.
//! It is what shows Claude Code's permission prompt as a raised hand. The same files also
//! say what background work a session has in flight (`signals.rs`), which the
//! handover reads.

pub mod requests;
mod signals;

use crate::delivery::text;
use crate::handover::work::Signal;
use crate::model::{Session, SessionState, ToolKind};
use crate::setup::HookEvent;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A file still being written is retried; one that never parses is dropped.
const WRITE_GRACE: Duration = Duration::from_secs(5);
/// An event this old says nothing about the session now.
const STALE: Duration = Duration::from_mins(30);
/// The longest notification message kept for a permission card.
const ASKED_MAX: usize = 300;
/// The most background-work signals held until they are taken.
const SIGNALS_MAX: usize = 2000;
/// A session start counts as idle only once it is this old, and gives way to a real event
/// dropped up to this long before it: the order of the two files is not promised.
const START_SETTLE: Duration = Duration::from_secs(10);
/// The `SessionStart` sources after which the session sits at its prompt (docs `hooks`, the
/// section on its input). `compact` comes in the middle of a turn and `fork` says nothing.
const STARTS_AT_PROMPT: [&str; 3] = ["startup", "resume", "clear"];
/// Claude Code's notification types that do not say the session waits on a person (docs
/// `hooks`, "Notification", read 2026-11-20).
const NOT_WAITING: [&str; 6] = [
    "auth_success",
    "auth_storage_failure",
    "elicitation_complete",
    "elicitation_response",
    "agent_completed",
    "plugin_notification",
];

/// The events each tool's feed hooks report. Codex has no `Notification` hook;
/// its `PermissionRequest` fires just before an approval prompt (docs, checked 2026-10-09).
/// Claude Code's `SessionStart`, `SubagentStop` and `StopFailure` are for the
/// handover: the first two say what background work is in flight, the last wakes
/// the building at an API error (docs `hooks`, read 2026-11-13).
#[must_use]
pub fn events(tool: ToolKind) -> &'static [HookEvent] {
    match tool {
        ToolKind::ClaudeCode => &[
            HookEvent::SessionStart,
            HookEvent::UserPromptSubmit,
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::Notification,
            HookEvent::SubagentStop,
            HookEvent::Stop,
            HookEvent::StopFailure,
        ],
        ToolKind::Codex => &[
            HookEvent::UserPromptSubmit,
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::PermissionRequest,
            HookEvent::Stop,
        ],
        _ => &[],
    }
}

/// The part of a hook's input the feed reads.
#[derive(Deserialize)]
struct Payload {
    session_id: String,
    /// Claude Code's kind of notification: `permission_prompt`, `idle_prompt`, ….
    #[serde(default)]
    notification_type: Option<String>,
    /// A notification's text, such as "Claude needs your permission".
    #[serde(default)]
    message: Option<String>,
    /// Why a `SessionStart` fired: `startup`, `resume`, `clear`, `compact` or `fork`.
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seen {
    state: SessionState,
    at: SystemTime,
    /// A start mark: `SessionStart` said the session came up at its prompt.
    start: bool,
}

/// Whether `new` takes the place of `old` as a session's newest event.
fn replaces(old: Seen, new: Seen) -> bool {
    // `a` minus `b`, zero when `a` is the earlier one.
    let gap = |a: SystemTime, b: SystemTime| a.duration_since(b).unwrap_or_default();
    match (old.start, new.start) {
        // A mark never displaces anything from the last ten seconds.
        (_, true) => gap(new.at, old.at) >= START_SETTLE,
        // A real event displaces a mark unless it is more than ten seconds older.
        (true, false) => gap(old.at, new.at) <= START_SETTLE,
        (false, false) => old.at <= new.at,
    }
}

/// The start mark an event at `at` makes: a `SessionStart` that left the session at its prompt.
fn start_mark(event: HookEvent, payload: &Payload, at: SystemTime) -> Option<Seen> {
    (event == HookEvent::SessionStart
        && payload
            .source
            .as_deref()
            .is_some_and(|s| STARTS_AT_PROMPT.contains(&s)))
    .then_some(Seen {
        state: SessionState::Idle,
        at,
        start: true,
    })
}

pub struct Feed {
    dir: PathBuf,
    latest: BTreeMap<String, Seen>,
    /// The newest event of every session ever seen, never pruned by age: a desk idle
    /// for an hour must still be known to be idle.
    last: BTreeMap<String, Seen>,
    /// For a session whose newest event is a notification waiting on A Human, its
    /// message, cleaned onto one line.
    asked: BTreeMap<String, String>,
    /// What the hooks said about background work, until taken.
    signals: Vec<Signal>,
    /// The permission requests no hook has seen settled.
    requests: requests::Requests,
}

/// The state an event puts its session in, if it says anything about it.
/// Claude Code's three `quota_auto_resume_*` notifications (docs `hooks`, "Notification"):
/// `fired` continues the task, `disabled` ends the wait without continuing, and only
/// `stale` waits for A Human to press Enter.
fn state_of(event: HookEvent, payload: &Payload) -> Option<SessionState> {
    let notification = payload.notification_type.as_deref();
    Some(match event {
        HookEvent::UserPromptSubmit | HookEvent::PostToolUse => SessionState::Thinking,
        HookEvent::PreToolUse => SessionState::Running,
        HookEvent::Notification
            if matches!(
                notification,
                Some("idle_prompt" | "quota_auto_resume_disabled")
            ) =>
        {
            SessionState::Idle
        }
        HookEvent::Notification if notification == Some("quota_auto_resume_fired") => {
            SessionState::Thinking
        }
        HookEvent::Notification if notification.is_some_and(|n| NOT_WAITING.contains(&n)) => {
            return None;
        }
        HookEvent::Notification => SessionState::WaitingHuman,
        HookEvent::Stop => SessionState::Idle,
        HookEvent::StopFailure => SessionState::Error,
        // A permission request is a request, not a state: the reconcile raises the hand from
        // the request itself (spec 5.5).
        HookEvent::SessionStart | HookEvent::SubagentStop | HookEvent::PermissionRequest => {
            return None;
        }
    })
}

impl Feed {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            latest: BTreeMap::new(),
            last: BTreeMap::new(),
            asked: BTreeMap::new(),
            signals: Vec::new(),
            requests: requests::Requests::default(),
        }
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The hook command for `event`: a POSIX shell line, with the folder quoted.
    #[must_use]
    pub fn command(&self, event: HookEvent) -> String {
        let name = event.name();
        let template = self.dir.join(format!("{name}.XXXXXX"));
        let quoted = format!("'{}'", template.to_string_lossy().replace('\'', r"'\''"));
        format!("mkdir -p \"$(dirname {quoted})\" && cat > \"$(mktemp {quoted})\"")
    }

    /// Read and delete the events dropped since the last call.
    pub fn collect(&mut self, now: SystemTime) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let at = entry.metadata().and_then(|m| m.modified()).unwrap_or(now);
            let age = now.duration_since(at).unwrap_or_default();
            let event = name.split('.').next().and_then(HookEvent::named);
            let text = fs::read_to_string(&path).ok();
            let payload = text
                .as_deref()
                .and_then(|text| serde_json::from_str::<Payload>(text).ok());
            match (event, payload) {
                (Some(event), Some(payload)) => {
                    if let Some(body) = text.as_deref() {
                        self.requests.note(event, body, at);
                    }
                    if let Some(signal) = text.as_deref().and_then(|t| signals::read(event, t, at))
                    {
                        self.signals.push(signal);
                    }
                    let seen = state_of(event, &payload)
                        .map(|state| Seen {
                            state,
                            at,
                            start: false,
                        })
                        .or_else(|| start_mark(event, &payload, at));
                    if let Some(seen) = seen {
                        self.note_state(event, seen, payload);
                    }
                }
                (Some(_), None) if age < WRITE_GRACE => continue,
                // Not an event file, or one that never became valid JSON.
                _ => {}
            }
            // Gone already is fine: nobody else writes or reads these files.
            let _ = fs::remove_file(&path);
        }
        self.requests.settle();
        self.latest
            .retain(|_, seen| now.duration_since(seen.at).unwrap_or_default() < STALE);
        if self.signals.len() > SIGNALS_MAX {
            self.signals.sort_by_key(signals::at);
            let over = self.signals.len() - SIGNALS_MAX;
            self.signals.drain(..over);
        }
    }

    /// The permission requests no hook has seen settled.
    #[must_use]
    pub fn requests(&self) -> &requests::Requests {
        &self.requests
    }

    pub fn requests_mut(&mut self) -> &mut requests::Requests {
        &mut self.requests
    }

    /// What the hooks said about background work since the last call, oldest first.
    pub fn take_signals(&mut self) -> Vec<Signal> {
        let mut taken = std::mem::take(&mut self.signals);
        taken.sort_by_key(signals::at);
        taken
    }

    /// Let an event set its session's state where it is the newest one.
    fn note_state(&mut self, event: HookEvent, seen: Seen, payload: Payload) {
        let newest = self
            .last
            .get(&payload.session_id)
            .is_none_or(|old| replaces(*old, seen));
        for map in [&mut self.latest, &mut self.last] {
            if map
                .get(&payload.session_id)
                .is_none_or(|old| replaces(*old, seen))
            {
                map.insert(payload.session_id.clone(), seen);
            }
        }
        if newest {
            self.note_asked(event, seen.state, payload);
        }
    }

    /// Keep the message of a session's newest event when it is a notification
    /// waiting on A Human, and forget it at any other newest event.
    fn note_asked(&mut self, event: HookEvent, state: SessionState, payload: Payload) {
        let message = payload
            .message
            .filter(|_| event == HookEvent::Notification && state == SessionState::WaitingHuman)
            .and_then(|m| text::line(&m, ASKED_MAX).ok());
        match message {
            Some(m) => self.asked.insert(payload.session_id, m),
            None => self.asked.remove(&payload.session_id),
        };
    }

    /// What the session's open notification says it waits on A Human for, if anything.
    #[must_use]
    pub fn asked(&self, session_id: &str) -> Option<&str> {
        self.asked.get(session_id).map(String::as_str)
    }

    /// The feed holds an event for this session: its state is reported as it happens.
    #[must_use]
    pub fn reports(&self, session_id: &str) -> bool {
        self.latest.contains_key(session_id)
    }

    /// The newest event ever collected for this session, however old: its state and when it happened.
    #[must_use]
    pub fn seen(&self, session_id: &str) -> Option<(SessionState, SystemTime)> {
        self.last.get(session_id).map(|seen| (seen.state, seen.at))
    }

    /// The session's newest event is a start mark less than `START_SETTLE` old.
    #[must_use]
    pub fn settling(&self, session_id: &str, now: SystemTime) -> bool {
        self.last.get(session_id).is_some_and(|seen| {
            // A mark in the future is not old.
            seen.start && !now.duration_since(seen.at).is_ok_and(|d| d >= START_SETTLE)
        })
    }

    /// Let each session's newest event set its state when it is newer than the
    /// session's own last activity.
    pub fn apply(&self, sessions: &mut [Session]) {
        for session in sessions {
            let Some(seen) = self.latest.get(&session.id) else {
                continue;
            };
            let last = chrono::DateTime::parse_from_rfc3339(&session.last_activity_at)
                .ok()
                .map(SystemTime::from);
            if last.is_none_or(|last| seen.at >= last) {
                session.state = seen.state;
            }
        }
    }
}

#[cfg(test)]
mod start_tests;

#[cfg(test)]
mod tests;
