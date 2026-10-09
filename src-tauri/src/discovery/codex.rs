//! Codex (`OpenAI`'s `codex` CLI) discovery. One on-disk source, written by
//! Codex itself:
//!
//! * `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`: the transcript,
//!   one JSON record per line (`{"timestamp","ordinal","type","payload"}`).
//!   `session_meta.source` is the plain string `"cli"`/`"vscode"` for a real
//!   top-level session; for a sub-agent thread spawned by Codex's multi-agent
//!   mode it is an object (`{"subagent":{"thread_spawn":{...}}}`) and the
//!   session also carries a non-null `parent_thread_id`. We drop those as
//!   desks, the same way Claude Code's sidechains are dropped: they describe
//!   helpers, not the session itself. Their spend is added to the top-level
//!   thread that spawned them. Verified against real rollouts on this Mac
//!   (2026-09-29), including a repo that runs review swarms through Codex's
//!   subagent feature.
//! * `~/.codex/session_index.jsonl`: `{"id","thread_name","updated_at"}`,
//!   append-only. `id` is the *root* session id (`session_meta.session_id`,
//!   stable across `codex resume`), not the per-rollout `session_meta.id`
//!   (which is the uuid embedded in the filename). We look titles up by the
//!   root id and keep the last (most recent) line per id.
//!
//! ## Liveness
//!
//! Codex has no equivalent of Claude Code's `~/.claude/sessions/<pid>.json`
//! registry, so liveness is inferred from `lsof`. Verified on this Mac
//! (2026-09-29) against two real `codex` processes:
//!
//! * A `codex` CLI running interactively in a terminal held its *own*
//!   top-level rollout open plus three more rollouts that turned out to be sub-agent threads it had spawned — the ones
//!   `session_meta.source` flags as sub-agent. Filtered to rollouts already
//!   known to be top-level (from the parse pass), exactly one open rollout
//!   remained: its own. A single `lsof -p <pid> -Ffn` call lists every open file,
//!   so this needs one `lsof` call per process, not one per candidate file.
//! * The `ChatGPT` desktop app's embedded `codex` binary (`comm` also just
//!   "codex" — matched by `ps` the same as the CLI) held zero rollout files
//!   open and had cwd `/`. Documented limitation: desktop-originated Codex
//!   threads are therefore never live, only ever shown finished, since their
//!   process holds no open file to tie them to a rollout.
//!
//! So: a rollout is live when a running `codex` process holds *that exact
//! file* open, filtered to top-level (non-sub-agent) rollouts under
//! `~/.codex/sessions/**/rollout-*.jsonl` — proof, not a guess.
//!
//! ### No rollout yet (2026-10-08)
//!
//! Checked against a real Codex 0.154.0 in a pseudo-terminal: the rollout is
//! created lazily, only once the first turn starts (about 10 s after the first
//! message). A `codex` sitting at its prompt, or blocked on a pre-turn question
//! such as "Do you trust the contents of this directory?", holds no rollout, so
//! it had no desk. Such a process now gets a placeholder desk
//! (`codex-new-<pid>`, "New Codex session", never resumable) until its rollout
//! appears and it becomes that session. Two kinds of `codex` process are not
//! sessions and never get one: cwd `/` (the desktop app's), and any whose
//! arguments name the `app-server` or `mcp-server` subcommand (IDE extensions
//! and the desktop app run those from a workspace). Nor is any `codex` started,
//! at any depth, by another `codex`: a session's browser tool runs `node_repl`,
//! which starts `codex sandbox` helpers in the session's folder (seen
//! 2026-10-09). A process holding more than one top-level rollout stays
//! ambiguous: not live, no placeholder.

use super::collect_jsonl;
use super::pricing::{self, CodexUsage};
use super::rules::{LastBlock, Signals};
use crate::model::Spend;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

mod process;
use process::{list_codex_pids, probe, resolve_liveness_by_open_files, rolloutless_sessions};

/// A `token_count` event's `info`: cumulative totals and the latest request's usage.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TokenCountInfo {
    total_token_usage: CodexUsage,
    last_token_usage: CodexUsage,
}

/// What one rollout says, reduced to what the office shows.
#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts read from the rollout, not a state machine"
)]
pub struct Transcript {
    pub cwd: Option<String>,
    /// The root session id (`session_meta.session_id`), for the title index.
    pub root_session_id: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub last_activity: Option<String>,
    /// Each `token_count` event's growth in `total_token_usage`, priced at the model then in use.
    pub spend: Spend,
    pub human_spoke_last: bool,
    pub last_block: LastBlock,
    pub reply_to_human: String,
    /// The newest `task_complete` carried an `error`.
    pub api_error: bool,
    pub open_tools: usize,
    /// `task_started` seen with no matching `task_complete` yet.
    pub turn_open: bool,
    /// `session_meta.source` was not a plain string: a sub-agent thread, not a real session.
    pub is_subagent: bool,
    /// The thread that spawned this one, for a sub-agent.
    pub parent_thread_id: Option<String>,
}

impl Transcript {
    /// A rollout that says nothing yet.
    fn blank() -> Self {
        Self {
            cwd: None,
            root_session_id: None,
            model: None,
            effort: None,
            last_activity: None,
            spend: Spend::default(),
            human_spoke_last: false,
            last_block: LastBlock::Nothing,
            reply_to_human: String::new(),
            api_error: false,
            open_tools: 0,
            turn_open: false,
            is_subagent: false,
            parent_thread_id: None,
        }
    }
}

fn message_text(payload: &Value) -> String {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

/// Reduce a rollout to its current facts.
#[expect(
    clippy::too_many_lines,
    reason = "one reducer over the rollout's record kinds; splitting it scatters the shared state"
)]
pub fn parse_transcript(reader: impl BufRead) -> Transcript {
    let mut t = Transcript::blank();
    let mut counted = CodexUsage::default();
    let mut open: HashSet<String> = HashSet::new();
    // A sub-agent or forked rollout's first record is its own `session_meta`; a later record
    // in the same file can be a *copy* of the parent's (source "cli", no subagent wrapper).
    // Identity, cwd and the subagent flag must come from the first one, or a copied parent
    // meta later in the file would relabel a subagent thread as a real top-level session.
    let mut meta_seen = false;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(rec) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(payload) = rec.get("payload") else {
            continue;
        };
        if let Some(at) = rec.get("timestamp").and_then(Value::as_str) {
            t.last_activity = Some(at.to_string());
        }
        let kind = rec.get("type").and_then(Value::as_str).unwrap_or_default();
        match kind {
            "session_meta" if !meta_seen => {
                meta_seen = true;
                if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                    t.cwd = Some(cwd.to_string());
                }
                t.root_session_id = payload
                    .get("session_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                t.is_subagent = !matches!(payload.get("source"), Some(Value::String(_)) | None);
                t.parent_thread_id = payload
                    .get("parent_thread_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            "turn_context" => {
                if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                    t.cwd = Some(cwd.to_string());
                }
                if let Some(model) = payload.get("model").and_then(Value::as_str) {
                    t.model = Some(model.to_string());
                }
                if let Some(effort) = payload.get("effort").and_then(Value::as_str) {
                    t.effort = Some(effort.to_string());
                }
            }
            "event_msg" => match payload.get("type").and_then(Value::as_str) {
                Some("task_started") => t.turn_open = true,
                Some("task_complete") => {
                    t.turn_open = false;
                    t.api_error = payload.get("error").is_some_and(|e| !e.is_null());
                }
                // Interrupting a turn writes `turn_aborted`, never a `task_complete` (41/41 real
                // aborts on this Mac). Without this, an interrupted live session shows Running forever.
                Some("turn_aborted") => t.turn_open = false,
                // Totals are cumulative and the same total is often re-sent (rate-limit
                // updates), so only the growth since the last one counted is priced.
                Some("token_count") => {
                    if let Some(info) = payload
                        .get("info")
                        .and_then(|i| TokenCountInfo::deserialize(i).ok())
                    {
                        let total = info.total_token_usage;
                        let long_context = info.last_token_usage.input > 272_000;
                        t.spend = t.spend
                            + pricing::codex_cost(
                                t.model.as_deref(),
                                total.since(counted),
                                long_context,
                            );
                        counted = total;
                    }
                }
                _ => {}
            },
            "response_item" => match payload.get("type").and_then(Value::as_str) {
                Some("message") => {
                    let role = payload
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let text = message_text(payload);
                    if role == "user" {
                        t.human_spoke_last = true;
                        t.reply_to_human.clear();
                    } else if role == "assistant" {
                        t.human_spoke_last = false;
                        t.last_block = LastBlock::Text;
                        if !t.reply_to_human.is_empty() {
                            t.reply_to_human.push('\n');
                        }
                        t.reply_to_human.push_str(&text);
                    }
                }
                Some("reasoning") => t.last_block = LastBlock::Thinking,
                Some("function_call" | "custom_tool_call") => {
                    t.last_block = LastBlock::ToolUse;
                    if let Some(id) = payload.get("call_id").and_then(Value::as_str) {
                        open.insert(id.to_string());
                    }
                }
                Some("function_call_output" | "custom_tool_call_output") => {
                    if let Some(id) = payload.get("call_id").and_then(Value::as_str) {
                        open.remove(id);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    t.open_tools = open.len();
    t
}

/// Turn a transcript plus liveness into the state rules' inputs.
#[must_use]
pub fn signals(t: &Transcript, live_pid: Option<i32>) -> Signals {
    Signals {
        live: live_pid.is_some(),
        busy: t.turn_open,
        question_waiting: false,
        pending_tool: t.open_tools > 0,
        human_spoke_last: t.human_spoke_last,
        api_error: t.api_error,
        last_block: t.last_block,
        reply_to_human: t.reply_to_human.clone(),
    }
}

/// One rollout entry from `~/.codex/session_index.jsonl`.
struct IndexEntry {
    id: String,
    thread_name: String,
}

fn parse_index_line(text: &str) -> Option<IndexEntry> {
    let v: Value = serde_json::from_str(text).ok()?;
    Some(IndexEntry {
        id: v.get("id")?.as_str()?.to_string(),
        thread_name: v.get("thread_name")?.as_str()?.to_string(),
    })
}

/// Thread titles by root session id. The file is append-only; a later line
/// for the same id (a rename) overrides an earlier one.
pub fn read_title_index(path: &Path) -> HashMap<String, String> {
    let Ok(text) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let mut index = HashMap::new();
    for entry in text.lines().filter_map(parse_index_line) {
        index.insert(entry.id, entry.thread_name);
    }
    index
}

/// One Codex session as discovered on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub session_id: String,
    pub transcript: Transcript,
    pub title: Option<String>,
    pub live_pid: Option<i32>,
    /// Whether it can be offered for resuming once it ends: false for a
    /// placeholder, which has no rollout to resume.
    pub resumable: bool,
}

/// Scans Codex's rollouts, re-reading one only when it changed.
pub struct CodexSource {
    home: PathBuf,
    /// Finished sessions stay visible this long after their last write.
    keep_finished: Duration,
    cache: HashMap<PathBuf, (SystemTime, u64, Transcript)>,
    live_pids: HashMap<String, i32>,
    /// The running `codex` pids; tests swap in none, so a real session on the Mac stays out.
    pub(crate) list_pids: fn() -> Vec<i32>,
}

impl CodexSource {
    #[must_use]
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            keep_finished: Duration::from_hours(12),
            cache: HashMap::new(),
            live_pids: HashMap::new(),
            list_pids: list_codex_pids,
        }
    }

    fn transcript(&mut self, path: &Path) -> Option<Transcript> {
        let meta = fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?;
        if let Some((m, len, t)) = self.cache.get(path)
            && *m == modified
            && *len == meta.len()
        {
            return Some(t.clone());
        }
        let parsed = parse_transcript(BufReader::new(fs::File::open(path).ok()?));
        self.cache
            .insert(path.to_path_buf(), (modified, meta.len(), parsed.clone()));
        Some(parsed)
    }

    /// Every live session, plus finished ones written to recently.
    pub fn scan(&mut self, now: SystemTime) -> Vec<Found> {
        self.scan_with(now, (self.list_pids)())
    }

    /// `scan`, with the running `codex` pids supplied instead of queried from the OS.
    pub fn scan_with(&mut self, now: SystemTime, pids: Vec<i32>) -> Vec<Found> {
        let codex_home = self.home.join(".codex");
        let title_index = read_title_index(&codex_home.join("session_index.jsonl"));

        let mut paths = Vec::new();
        collect_jsonl(&codex_home.join("sessions"), &mut paths);

        let mut parsed: Vec<(PathBuf, String, SystemTime, Transcript)> = Vec::new();
        let mut parent_of: HashMap<String, String> = HashMap::new();
        let mut subagent_spend: Vec<(String, Spend)> = Vec::new();
        for path in paths {
            let Some(session_id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(session_uuid)
            else {
                continue;
            };
            let Ok(mtime) = fs::metadata(&path).and_then(|m| m.modified()) else {
                continue;
            };
            let Some(transcript) = self.transcript(&path) else {
                continue;
            };
            if transcript.is_subagent {
                if let Some(parent) = transcript.parent_thread_id {
                    parent_of.insert(session_id.clone(), parent);
                    subagent_spend.push((session_id, transcript.spend));
                }
                continue;
            }
            if transcript.cwd.is_none() {
                continue;
            }
            parsed.push((path, session_id, mtime, transcript));
        }

        // A sub-agent's spend is its top-level thread's: walk up through nested sub-agents.
        let mut spend_by_thread: HashMap<String, Spend> = HashMap::new();
        for (id, spend) in subagent_spend {
            let mut at = &id;
            // Bounded, so a malformed parent cycle cannot hang the scan.
            for _ in 0..parent_of.len() {
                let Some(parent) = parent_of.get(at) else {
                    break;
                };
                at = parent;
            }
            let total = spend_by_thread.entry(at.clone()).or_default();
            *total = *total + spend;
        }
        for (_, id, _, transcript) in &mut parsed {
            if let Some(extra) = spend_by_thread.get(id) {
                transcript.spend = transcript.spend + *extra;
            }
        }

        let top_level: HashSet<String> = parsed.iter().map(|(_, id, _, _)| id.clone()).collect();
        let sessions_dir = codex_home.join("sessions");
        let process_open: Vec<_> = pids
            .into_iter()
            .map(|pid| probe(pid, &sessions_dir, &top_level))
            .collect();
        self.live_pids = resolve_liveness_by_open_files(&process_open);
        let placeholders: Vec<Found> = rolloutless_sessions(&process_open)
            .into_iter()
            .map(|(pid, cwd)| {
                let session_id = format!("codex-new-{pid}");
                self.live_pids.insert(session_id.clone(), pid);
                Found {
                    session_id,
                    transcript: Transcript {
                        cwd: Some(cwd),
                        ..Transcript::blank()
                    },
                    title: Some("New Codex session".to_string()),
                    live_pid: Some(pid),
                    resumable: false,
                }
            })
            .collect();

        parsed
            .into_iter()
            .filter(|(_, id, mtime, _)| {
                self.live_pids.contains_key(id)
                    || now
                        .duration_since(*mtime)
                        .is_ok_and(|age| age <= self.keep_finished)
            })
            .map(|(_, session_id, _, transcript)| {
                let title = transcript
                    .root_session_id
                    .as_ref()
                    .and_then(|id| title_index.get(id))
                    .cloned();
                let live_pid = self.live_pids.get(&session_id).copied();
                Found {
                    session_id,
                    transcript,
                    title,
                    live_pid,
                    resumable: true,
                }
            })
            .chain(placeholders)
            .collect()
    }

    /// Pid of a running session, for focusing its terminal.
    #[must_use]
    pub fn pid_of(&self, session_id: &str) -> Option<i32> {
        self.live_pids.get(session_id).copied()
    }
}

/// The uuid a rollout filename ends with, e.g.
/// `rollout-2026-09-29T10-00-00-deadbeef-0000-4000-8000-000000000000` ->
/// `deadbeef-0000-4000-8000-000000000000`. A uuid is 5 `-`-separated groups
/// of hex digits (8-4-4-4-12); we take the last 5 groups of the filename.
fn session_uuid(file_stem: &str) -> Option<String> {
    let groups: Vec<&str> = file_stem.rsplit('-').take(5).collect();
    if groups.len() != 5
        || groups
            .iter()
            .any(|g| !g.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return None;
    }
    let lens = [12, 4, 4, 4, 8];
    if groups.iter().zip(lens).any(|(g, want)| g.len() != want) {
        return None;
    }
    let mut rev: Vec<&str> = groups;
    rev.reverse();
    Some(rev.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse(lines: &[&str]) -> Transcript {
        parse_transcript(Cursor::new(lines.join("\n")))
    }

    const META: &str = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"s1","session_id":"root1","cwd":"/code/app","source":"cli"}}"#;
    const TURN_CTX: &str = r#"{"timestamp":"2026-09-29T10:00:01.000Z","type":"turn_context","payload":{"cwd":"/code/app","model":"gpt-5-codex","effort":"high"}}"#;
    const USER: &str = r#"{"timestamp":"2026-09-29T10:00:02.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix the parser"}]}}"#;

    #[test]
    fn reads_cwd_model_effort_and_reply() {
        let started = r#"{"timestamp":"2026-09-29T10:00:03.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
        let reasoning = r#"{"timestamp":"2026-09-29T10:00:04.000Z","type":"response_item","payload":{"type":"reasoning","summary":[]}}"#;
        let reply = r#"{"timestamp":"2026-09-29T10:00:05.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done. Tests pass."}]}}"#;
        let complete = r#"{"timestamp":"2026-09-29T10:00:06.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","last_agent_message":"Done."}}"#;
        let tokens = r#"{"timestamp":"2026-09-29T10:00:06.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":40,"cached_input_tokens":10,"output_tokens":2,"total_tokens":42}}}}"#;
        let t = parse(&[
            META, TURN_CTX, USER, started, reasoning, reply, complete, tokens,
        ]);
        assert_eq!(t.cwd.as_deref(), Some("/code/app"));
        assert_eq!(t.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(t.effort.as_deref(), Some("high"));
        assert_eq!(t.root_session_id.as_deref(), Some("root1"));
        assert_eq!(t.reply_to_human, "Done. Tests pass.");
        assert_eq!(t.spend.tokens, 32);
        assert!(!t.turn_open);
        assert!(!t.api_error);
        assert_eq!(t.last_activity.as_deref(), Some("2026-09-29T10:00:06.000Z"));
        assert!(!t.human_spoke_last);
        assert!(!t.is_subagent);
    }

    #[test]
    fn turn_in_progress_until_task_complete() {
        let started = r#"{"timestamp":"2026-09-29T10:00:03.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
        let t = parse(&[META, USER, started]);
        assert!(t.turn_open);

        let complete = r#"{"timestamp":"2026-09-29T10:00:06.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}"#;
        let done = parse(&[META, USER, started, complete]);
        assert!(!done.turn_open);
        assert!(!done.api_error);
    }

    #[test]
    fn task_complete_error_is_flagged() {
        let complete = r#"{"timestamp":"2026-09-29T10:00:06.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","error":{"message":"usage limit","codex_error_info":"usage_limit_exceeded"}}}"#;
        let t = parse(&[META, USER, complete]);
        assert!(t.api_error);
    }

    #[test]
    fn open_function_call_counts_as_pending() {
        let call = r#"{"timestamp":"2026-09-29T10:00:03.000Z","type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell","arguments":"{}"}}"#;
        let t = parse(&[META, USER, call]);
        assert_eq!(t.open_tools, 1);
        assert_eq!(t.last_block, LastBlock::ToolUse);

        let output = r#"{"timestamp":"2026-09-29T10:00:04.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"ok"}}"#;
        let closed = parse(&[META, USER, call, output]);
        assert_eq!(closed.open_tools, 0);
    }

    #[test]
    fn a_trailing_prose_question_scores_as_a_soft_block() {
        let reply = r#"{"timestamp":"2026-09-29T10:00:05.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Fixed it.\n\nShould I also bump the version?"}]}}"#;
        let t = parse(&[META, USER, reply]);
        let s = signals(&t, Some(1));
        assert_eq!(
            crate::discovery::rules::derive_state(&s),
            crate::model::SessionState::WaitingHuman
        );
        assert_eq!(
            super::super::rules::score_soft_block(&t.reply_to_human).question,
            "Should I also bump the version?"
        );
    }

    #[test]
    fn subagent_thread_is_flagged_and_dropped() {
        let sub_meta = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"s2","session_id":"root1","cwd":"/code/app","source":{"subagent":{"thread_spawn":{"parent_thread_id":"root1"}}}}}"#;
        let t = parse(&[sub_meta]);
        assert!(t.is_subagent);
    }

    #[test]
    fn human_speaking_again_clears_the_prior_reply() {
        let reply = r#"{"timestamp":"2026-09-29T10:00:05.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Should I proceed?"}]}}"#;
        let next_human = r#"{"timestamp":"2026-09-29T10:01:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"yes"}]}}"#;
        let t = parse(&[META, USER, reply, next_human]);
        assert!(t.human_spoke_last);
        assert_eq!(t.reply_to_human, "");
    }

    #[test]
    fn title_index_keeps_the_last_line_per_id() {
        let dir = std::env::temp_dir().join(format!("ob-codex-index-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("session_index.jsonl");
        fs::write(
            &path,
            "{\"id\":\"root1\",\"thread_name\":\"Example thread one\",\"updated_at\":\"2000-01-01T00:00:00Z\"}\n\
             {\"id\":\"root1\",\"thread_name\":\"Example thread renamed\",\"updated_at\":\"2000-01-01T00:05:00Z\"}\n\
             not json\n",
        )
        .expect("write");
        let index = read_title_index(&path);
        fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(
            index.get("root1").map(String::as_str),
            Some("Example thread renamed")
        );
    }

    #[test]
    fn cache_reuses_a_parsed_transcript_when_the_file_is_unchanged() {
        let dir = std::env::temp_dir().join(format!("ob-codex-cache-{}", std::process::id()));
        fs::create_dir_all(dir.join(".codex/sessions/2026/09/29")).expect("temp dir");
        let path = dir.join(".codex/sessions/2026/09/29/rollout-2026-09-29T10-00-00-deadbeef-0000-4000-8000-000000000000.jsonl");
        fs::write(&path, format!("{META}\n{TURN_CTX}\n{USER}\n")).expect("write");

        let mut source = CodexSource::new(dir.clone());
        let first = source.scan_with(SystemTime::now(), Vec::new());
        assert_eq!(first.len(), 1);
        assert_eq!(source.cache.len(), 1);
        let cached_before = source.cache.values().next().cloned();

        let second = source.scan_with(SystemTime::now(), Vec::new());
        assert_eq!(second.len(), 1);
        assert_eq!(source.cache.values().next().cloned(), cached_before);

        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn spend_prices_each_token_count_once_at_the_model_then_in_use() {
        let ctx = |model: &str| {
            format!(
                r#"{{"timestamp":"t","type":"turn_context","payload":{{"cwd":"/code/app","model":"{model}"}}}}"#
            )
        };
        let count = |input: u64, output: u64| {
            format!(
                r#"{{"timestamp":"t","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":{output}}},"last_token_usage":{{"input_tokens":1}}}}}}}}"#
            )
        };
        let astra = ctx("gpt-6-astra");
        let luna = ctx("gpt-6-luna");
        let (first, resent, second) = (
            count(1_000_000, 0),
            count(1_000_000, 0),
            count(2_000_000, 1_000_000),
        );
        let t = parse(&[META, &astra, &first, &resent, &luna, &second]);
        // 1M input on astra ($10), then 1M input + 1M output on luna ($0.10 + $0.50).
        assert!((t.spend.usd - 10.6).abs() < 1e-9, "{}", t.spend.usd);
        assert_eq!(t.spend.tokens, 3_000_000);
        assert_eq!(t.spend.unpriced_tokens, 0);
    }

    #[test]
    fn a_thread_spends_what_its_sub_agents_spend_at_any_depth() {
        let dir = std::env::temp_dir().join(format!("ob-codex-subagents-{}", std::process::id()));
        let day = dir.join(".codex/sessions/2026/09/29");
        fs::create_dir_all(&day).expect("temp dir");
        let uuid = |n: u32| format!("deadbeef-0000-4000-8000-{n:012}");
        let meta = |id: &str, parent: Option<&str>| match parent {
            None => format!(
                r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","cwd":"/code/app","source":"cli"}}}}"#
            ),
            Some(p) => format!(
                r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","parent_thread_id":"{p}","cwd":"/code/app","source":{{"subagent":{{"other":"guardian"}}}}}}}}"#
            ),
        };
        let spent = r#"{"timestamp":"t","type":"turn_context","payload":{"cwd":"/code/app","model":"gpt-6-luna"}}
{"timestamp":"t","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000000,"output_tokens":0}}}}"#;
        let (top, child, grandchild) = (uuid(1), uuid(2), uuid(3));
        for (id, parent) in [
            (&top, None),
            (&child, Some(top.as_str())),
            (&grandchild, Some(child.as_str())),
        ] {
            fs::write(
                day.join(format!("rollout-2026-09-29T10-00-00-{id}.jsonl")),
                format!("{}\n{spent}\n", meta(id, parent)),
            )
            .expect("write");
        }

        let found = CodexSource::new(dir.clone()).scan_with(SystemTime::now(), Vec::new());
        assert_eq!(found.len(), 1);
        // Three threads of 1M input tokens on gpt-6-luna at $0.10 per MTok.
        assert!(
            (found[0].transcript.spend.usd - 0.3).abs() < 1e-9,
            "{}",
            found[0].transcript.spend.usd
        );
        assert_eq!(found[0].transcript.spend.tokens, 3_000_000);
        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn session_uuid_reads_the_filename_tail() {
        assert_eq!(
            session_uuid("rollout-2026-09-29T10-00-00-deadbeef-0000-4000-8000-000000000000"),
            Some("deadbeef-0000-4000-8000-000000000000".to_string())
        );
        assert_eq!(session_uuid("not-a-uuid"), None);
    }

    // VERIFIER (failing on purpose): Codex writes `turn_aborted` (no `task_complete`) when the
    // user interrupts a turn. 41/41 aborts in real rollouts on this Mac had no later task_complete.
    #[test]
    fn verifier_turn_aborted_closes_the_turn() {
        let started = r#"{"timestamp":"2026-09-29T10:00:03.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
        let aborted = r#"{"timestamp":"2026-09-29T10:00:09.000Z","type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t1","reason":"interrupted"}}"#;
        let t = parse(&[META, USER, started, aborted]);
        assert!(
            !t.turn_open,
            "an interrupted turn is over; live session would show Running forever"
        );
    }

    // VERIFIER (failing on purpose): a sub-agent/forked rollout carries its own session_meta first
    // and a copy of the parent's (source "cli") later. The first one must decide.
    #[test]
    fn verifier_subagent_with_copied_parent_meta_stays_a_subagent() {
        let sub_meta = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"s2","session_id":"root1","cwd":"/code/app","source":{"subagent":{"thread_spawn":{"parent_thread_id":"root1"}}}}}"#;
        let parent_meta = r#"{"timestamp":"2026-09-29T10:00:00.500Z","type":"session_meta","payload":{"id":"root1","session_id":"root1","cwd":"/code/app","source":"cli"}}"#;
        let t = parse(&[sub_meta, parent_meta]);
        assert!(t.is_subagent);
    }

    // A running codex with no rollout (here this very test process, standing in for one, as
    // `lsof` and `ps` really are run on it) gets a placeholder desk that is live but not resumable.
    #[test]
    fn a_running_process_with_no_rollout_gets_a_placeholder_session() {
        let dir = std::env::temp_dir().join(format!("ob-codex-placeholder-{}", std::process::id()));
        fs::create_dir_all(dir.join(".codex/sessions")).expect("temp dir");
        let pid = i32::try_from(std::process::id()).expect("pid fits");

        let found = CodexSource::new(dir.clone()).scan_with(SystemTime::now(), vec![pid]);
        fs::remove_dir_all(&dir).expect("cleanup");

        let [placeholder] = found.as_slice() else {
            panic!("expected one placeholder, got {found:?}");
        };
        assert_eq!(placeholder.session_id, format!("codex-new-{pid}"));
        assert_eq!(placeholder.title.as_deref(), Some("New Codex session"));
        assert_eq!(placeholder.live_pid, Some(pid));
        assert!(!placeholder.resumable);
        assert_eq!(
            placeholder.transcript.cwd.as_deref(),
            std::env::current_dir()
                .ok()
                .and_then(|d| d.to_str().map(str::to_string))
                .as_deref()
        );
    }
}
