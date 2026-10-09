//! The hook status feed: agents the building launches get hooks that drop each
//! event into a folder as its own file (`<Event>.XXXXXX`, the hook's JSON from
//! standard input). One file per event means hooks running side by side never
//! interleave, and the hook needs only `sh`, `mktemp` and `cat`. A refresh
//! collects the files, deletes them, and lets the newest event of a session set
//! its desk's state when it is newer than what the session's own files say.
//! It is what shows a permission prompt as a raised hand.

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

/// The events each tool's feed hooks report. Codex has no `Notification` hook;
/// its `PermissionRequest` fires just before an approval prompt (docs, checked 2026-10-09).
#[must_use]
pub fn events(tool: ToolKind) -> &'static [HookEvent] {
    match tool {
        ToolKind::ClaudeCode => &[
            HookEvent::UserPromptSubmit,
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::Notification,
            HookEvent::Stop,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seen {
    state: SessionState,
    at: SystemTime,
}

pub struct Feed {
    dir: PathBuf,
    latest: BTreeMap<String, Seen>,
}

fn state_of(event: HookEvent, payload: &Payload) -> SessionState {
    match event {
        HookEvent::UserPromptSubmit | HookEvent::PostToolUse | HookEvent::SessionStart => {
            SessionState::Thinking
        }
        HookEvent::PreToolUse => SessionState::Running,
        HookEvent::Notification if payload.notification_type.as_deref() == Some("idle_prompt") => {
            SessionState::Idle
        }
        HookEvent::Notification | HookEvent::PermissionRequest => SessionState::WaitingHuman,
        HookEvent::Stop => SessionState::Idle,
    }
}

impl Feed {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            latest: BTreeMap::new(),
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
            let payload = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Payload>(&text).ok());
            match (event, payload) {
                (Some(event), Some(payload)) => {
                    let seen = Seen {
                        state: state_of(event, &payload),
                        at,
                    };
                    let newer = self
                        .latest
                        .get(&payload.session_id)
                        .is_none_or(|old| old.at <= at);
                    if newer {
                        self.latest.insert(payload.session_id, seen);
                    }
                }
                (Some(_), None) if age < WRITE_GRACE => continue,
                // Not an event file, or one that never became valid JSON.
                _ => {}
            }
            // Gone already is fine: nobody else writes or reads these files.
            let _ = fs::remove_file(&path);
        }
        self.latest
            .retain(|_, seen| now.duration_since(seen.at).unwrap_or_default() < STALE);
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
mod tests {
    use super::*;
    use crate::model::{ControlMode, Spend};
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn feed(tag: &str) -> Feed {
        let dir = std::env::temp_dir().join(format!("ob-feed-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Feed::new(dir)
    }

    /// Run the hook command the way a tool would: through `sh`, with the event on stdin.
    fn fire(feed: &Feed, event: HookEvent, json: &str) {
        let mut child = Command::new("/bin/sh")
            .args(["-c", &feed.command(event)])
            .stdin(Stdio::piped())
            .spawn()
            .expect("sh");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(json.as_bytes())
            .expect("write");
        assert!(child.wait().expect("wait").success());
    }

    fn session(id: &str, last: &str) -> Session {
        Session {
            id: id.to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: "p".to_string(),
            title: "t".to_string(),
            state: SessionState::Running,
            control: ControlMode::Full,
            model: None,
            effort: None,
            last_activity_at: last.to_string(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers: Vec::new(),
            pets: Vec::new(),
            worktree: false,
        }
    }

    #[test]
    fn a_permission_prompt_raises_the_agents_hand_until_it_moves_on() {
        let mut feed = feed("prompt");
        fire(
            &feed,
            HookEvent::Notification,
            r#"{"session_id":"s1","notification_type":"permission_prompt","message":"Claude needs your permission"}"#,
        );
        feed.collect(SystemTime::now());
        assert_eq!(
            fs::read_dir(feed.dir()).expect("dir").count(),
            0,
            "files are taken"
        );
        let mut sessions = vec![
            session("s1", "2000-01-01T00:00:00Z"),
            session("s2", "2000-01-01T00:00:00Z"),
        ];
        feed.apply(&mut sessions);
        assert_eq!(sessions[0].state, SessionState::WaitingHuman);
        assert_eq!(
            sessions[1].state,
            SessionState::Running,
            "no events: unchanged"
        );
        fire(
            &feed,
            HookEvent::PreToolUse,
            r#"{"session_id":"s1","tool_name":"Bash"}"#,
        );
        feed.collect(SystemTime::now());
        feed.apply(&mut sessions);
        assert_eq!(sessions[0].state, SessionState::Running);
    }

    #[test]
    fn a_session_whose_files_are_newer_keeps_their_state() {
        let mut feed = feed("older");
        fire(&feed, HookEvent::Stop, r#"{"session_id":"s1"}"#);
        feed.collect(SystemTime::now());
        let mut sessions = vec![session("s1", "2999-01-01T00:00:00Z")];
        feed.apply(&mut sessions);
        assert_eq!(sessions[0].state, SessionState::Running);
    }

    #[test]
    fn a_file_still_being_written_is_left_for_the_next_look() {
        let mut feed = feed("partial");
        fs::create_dir_all(feed.dir()).expect("mkdir");
        fs::write(feed.dir().join("Stop.abc123"), r#"{"session_"#).expect("write");
        fs::write(feed.dir().join("notes.txt"), "not an event").expect("write");
        feed.collect(SystemTime::now());
        assert!(feed.dir().join("Stop.abc123").exists());
        assert!(!feed.dir().join("notes.txt").exists());
        feed.collect(SystemTime::now() + WRITE_GRACE + Duration::from_secs(1));
        assert!(
            !feed.dir().join("Stop.abc123").exists(),
            "dropped once it can no longer be in flight"
        );
    }

    // Codex fires `PermissionRequest` just before its approval prompt, and nothing in its rollout
    // shows the wait (a hire sat on "Would you like to run the following command?" as Working).
    #[test]
    fn a_codex_approval_prompt_raises_the_agents_hand() {
        assert!(events(ToolKind::Codex).contains(&HookEvent::PermissionRequest));
        let mut feed = feed("codex-approval");
        fire(
            &feed,
            HookEvent::PreToolUse,
            r#"{"session_id":"c1","tool_name":"Bash"}"#,
        );
        fire(
            &feed,
            HookEvent::PermissionRequest,
            r#"{"session_id":"c1","tool_name":"Bash","tool_input":{"command":"node /tmp/review.ts","description":null}}"#,
        );
        feed.collect(SystemTime::now() + Duration::from_millis(10));
        let mut sessions = vec![session("c1", "2000-01-01T00:00:00Z")];
        sessions[0].tool = ToolKind::Codex;
        feed.apply(&mut sessions);
        assert_eq!(sessions[0].state, SessionState::WaitingHuman);
    }

    #[test]
    fn codex_gets_no_notification_hook() {
        assert!(!events(ToolKind::Codex).contains(&HookEvent::Notification));
        assert!(events(ToolKind::ClaudeCode).contains(&HookEvent::Notification));
        assert!(events(ToolKind::Opencode).is_empty());
    }
}
