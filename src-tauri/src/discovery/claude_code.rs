//! Claude Code discovery. Two on-disk sources, both written by Claude Code itself:
//!
//! * `~/.claude/sessions/<pid>.json`: one file per running interactive session
//!   (`pid`, `sessionId`, `cwd`, `status` busy/idle). A live pid there is how we
//!   know a session is running, including ones started by hand in a terminal.
//!   Claude Code writes `<pid>.<token>.key` as it starts and `<pid>.json` only
//!   once past any startup prompt, such as "Do you trust this folder?" (seen on
//!   Claude Code 2.1.296, 2026-10-13). A live pid with a `.key` and no `.json`
//!   is a session stuck there: it gets a placeholder desk (`claude-new-<pid>`)
//!   so its terminal can be opened and the prompt answered.
//! * `~/.claude/projects/<dir>/<sessionId>.jsonl`: the transcript. One JSON
//!   record per line; we read the title, model, effort, token usage, open tool
//!   calls and any unanswered `AskUserQuestion`.

use super::collect_jsonl;
use super::pets::MonitorCall;
use super::pricing;
use super::rules::{LastBlock, Signals};
use crate::model::{Helper, SessionState, Spend};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A running session from Claude Code's own registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveEntry {
    pub pid: i32,
    pub session_id: String,
    pub cwd: String,
    pub busy: bool,
    /// Blocked on the human (`status: "waiting"`). Claude Code writes an open
    /// `AskUserQuestion` or permission prompt to the transcript only once it is
    /// answered, so this is the only sign of one.
    pub waiting: bool,
    pub name: Option<String>,
}

/// An `AskUserQuestion` call that has no answer yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingQuestion {
    pub tool_use_id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub asked_at: String,
}

/// What one transcript says, reduced to what the office shows.
#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts read from the transcript, not a state machine"
)]
pub struct Transcript {
    /// The folder it started in: its floor.
    pub cwd: Option<String>,
    /// The folder its shell is in now, after any `cd`.
    pub latest_cwd: Option<String>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub last_activity: Option<String>,
    /// Priced once per API message; tokens are input, cache-creation and output.
    pub spend: Spend,
    /// The human has spoken since the agent last replied: the agent owes the next move.
    pub human_spoke_last: bool,
    pub last_block: LastBlock,
    /// The agent's latest reply to the human. Turns started by the system (task
    /// notifications, peer messages) never replace it, so an open question
    /// survives the agent answering a notification in between.
    pub reply_to_human: String,
    pub api_error: bool,
    pub open_tools: usize,
    pub question: Option<PendingQuestion>,
    /// The agent signed off: its last reply ended the turn, or (a workflow
    /// agent) its `StructuredOutput` result came back.
    pub finished: bool,
    /// Started by a script or the SDK (`claude -p`, entrypoint `sdk-*`), not by a person.
    pub scripted: bool,
    /// Its `Monitor` calls, so a running one shows as a monitor pet, not a shell.
    pub monitors: Vec<MonitorCall>,
}

/// Whether a process exists (signal 0 probes without sending anything).
#[must_use]
#[expect(
    unsafe_code,
    reason = "kill(2) has no safe std equivalent; the call only probes"
)]
pub fn pid_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    // SAFETY: kill(2) with signal 0 performs only the existence/permission check.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Ask a process to exit cleanly (SIGTERM). Whether it did is for [`pid_alive`] to tell.
#[expect(
    unsafe_code,
    reason = "kill(2) has no safe std equivalent; the call only sends SIGTERM"
)]
pub fn terminate(pid: i32) {
    if pid > 0 {
        // SAFETY: kill(2) with SIGTERM touches no memory; a failure only means the process is gone.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

/// Parse one registry file. Returns None for anything that is not a session record.
pub fn parse_registry_entry(text: &str) -> Option<LiveEntry> {
    let v: Value = serde_json::from_str(text).ok()?;
    let pid = i32::try_from(v.get("pid")?.as_i64()?).ok()?;
    Some(LiveEntry {
        pid,
        session_id: v.get("sessionId")?.as_str()?.to_string(),
        cwd: v.get("cwd")?.as_str()?.to_string(),
        busy: v.get("status").and_then(Value::as_str) == Some("busy"),
        waiting: v.get("status").and_then(Value::as_str) == Some("waiting"),
        name: v.get("name").and_then(Value::as_str).map(str::to_string),
    })
}

/// How long a `.key` may stand without its `.json` before its session counts as
/// stuck at a startup prompt: both are written within a moment of each other.
const UNREGISTERED_AFTER: Duration = Duration::from_secs(3);

/// Starts the placeholder session id of a Claude Code process that has not registered.
const UNREGISTERED_PREFIX: &str = "claude-new-";

/// The placeholder session id of a Claude Code process that has not registered.
#[must_use]
pub fn unregistered_id(pid: i32) -> String {
    format!("{UNREGISTERED_PREFIX}{pid}")
}

/// Running Claude Code processes in `dir` (`~/.claude/sessions`) that wrote
/// their `.key` at least [`UNREGISTERED_AFTER`] before `now` and no `.json`.
pub fn unregistered_pids(dir: &Path, now: SystemTime, alive: impl Fn(i32) -> bool) -> Vec<i32> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut pids: Vec<i32> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "key"))
        .filter(|p| !written_within(p, now, UNREGISTERED_AFTER))
        .filter_map(|p| p.file_name()?.to_str()?.split('.').next()?.parse().ok())
        .filter(|pid| !dir.join(format!("{pid}.json")).exists() && alive(*pid))
        .collect();
    pids.sort_unstable();
    pids
}

/// Running sessions from `~/.claude/sessions`, dropping entries whose process is gone.
pub fn read_registry(dir: &Path, alive: impl Fn(i32) -> bool) -> Vec<LiveEntry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| fs::read_to_string(p).ok())
        .filter_map(|t| parse_registry_entry(&t))
        .filter(|e| alive(e.pid))
        .collect()
}

fn ask_user_question(input: &Value, id: &str, at: &str) -> Option<PendingQuestion> {
    let q = input.get("questions")?.as_array()?.first()?;
    let options = q
        .get("options")
        .and_then(Value::as_array)
        .map(|opts| {
            opts.iter()
                .filter_map(|o| o.get("label").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(PendingQuestion {
        tool_use_id: id.to_string(),
        prompt: q.get("question")?.as_str()?.to_string(),
        options,
        asked_at: at.to_string(),
    })
}

/// Consume lines from `reader`, starting `start_offset` bytes into the
/// underlying file, call `each` with the offset each line starts at and the
/// line without its line ending, and return the offset just past the last line
/// consumed. When `defer_trailing_partial` is set, a trailing line with no
/// newline yet is left unread (the file may still be mid-write, so the
/// next call can pick it up once it is whole); otherwise it is processed
/// as-is, since its content is already fully present in `reader` even
/// though the source text happens not to end in a newline (e.g. a full
/// parse of an in-memory buffer, or a split made at a line boundary).
/// `ClaudeSource` always passes `true`, so a transcript whose very last
/// line on disk never gains a trailing newline is never picked up by the
/// incremental path; every real transcript observed ends in `\n`, so this
/// is accepted rather than worked around.
fn feed_lines(
    mut reader: impl BufRead,
    start_offset: u64,
    defer_trailing_partial: bool,
    mut each: impl FnMut(u64, &str),
) -> u64 {
    let mut offset = start_offset;
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = match reader.read_line(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let complete = buf.ends_with('\n');
        if !complete && defer_trailing_partial {
            break;
        }
        let at = offset;
        offset += u64::try_from(n).unwrap_or(u64::MAX);
        let line = buf.strip_suffix('\n').unwrap_or(&buf);
        let line = line.strip_suffix('\r').unwrap_or(line);
        each(at, line);
        if !complete {
            break;
        }
    }
    offset
}

/// Resumable reduction of a transcript. Every field carried across lines in the
/// old single-pass reducer lives here, so feeding lines 1..n in one call or in
/// 1..k then k+1..n calls yields the same result: `parse_transcript` (the full
/// parse) and `ClaudeSource`'s incremental scans (`feed_lines` called again
/// with only the file's new bytes) are both thin wrappers around this state.
#[derive(Debug)]
struct ParserState {
    t: Transcript,
    open: HashSet<String>,
    usage_by_message: HashMap<String, Spend>,
    reply_message: Option<String>,
    /// The latest `StructuredOutput` call: its result is a workflow agent signing off.
    structured_output: Option<String>,
    in_human_turn: bool,
    /// A sub-agent's own transcript is all sidechain records; read it for its spend.
    keep_sidechains: bool,
}

impl ParserState {
    fn new() -> Self {
        Self {
            t: Transcript {
                cwd: None,
                latest_cwd: None,
                title: None,
                model: None,
                effort: None,
                last_activity: None,
                spend: Spend::default(),
                human_spoke_last: false,
                last_block: LastBlock::Nothing,
                reply_to_human: String::new(),
                api_error: false,
                open_tools: 0,
                question: None,
                finished: false,
                scripted: false,
                monitors: Vec::new(),
            },
            open: HashSet::new(),
            usage_by_message: HashMap::new(),
            reply_message: None,
            structured_output: None,
            // Transcripts from before prompts carried an origin count every prompt as human.
            in_human_turn: true,
            keep_sidechains: false,
        }
    }

    /// [`feed_lines`] into this state.
    fn feed_lines(&mut self, reader: impl BufRead, start_offset: u64, defer: bool) -> u64 {
        feed_lines(reader, start_offset, defer, |_, line| {
            self.process_line(line);
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one reducer over the transcript's record kinds; splitting it scatters the shared state"
    )]
    fn process_line(&mut self, line: &str) {
        let Ok(rec) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let kind = rec.get("type").and_then(Value::as_str).unwrap_or_default();
        if kind == "ai-title" {
            if let Some(title) = rec.get("aiTitle").and_then(Value::as_str) {
                self.t.title = Some(title.to_string());
            }
            return;
        }
        if kind != "assistant" && kind != "user" {
            return;
        }
        if !self.keep_sidechains && rec.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return;
        }
        // A `cd` later on does not move it off the floor it started on.
        if let Some(cwd) = rec.get("cwd").and_then(Value::as_str) {
            self.t.cwd.get_or_insert_with(|| cwd.to_string());
            self.t.latest_cwd = Some(cwd.to_string());
        }
        if let Some(entry) = rec.get("entrypoint").and_then(Value::as_str) {
            self.t.scripted = entry.starts_with("sdk");
        }
        let at = rec
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !at.is_empty() {
            self.t.last_activity = Some(at.clone());
        }
        let Some(message) = rec.get("message") else {
            return;
        };

        if kind == "assistant" {
            if self.in_human_turn {
                self.t.human_spoke_last = false;
            }
            self.t.api_error = rec.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true);
            self.t.finished = matches!(
                message.get("stop_reason").and_then(Value::as_str),
                Some("end_turn" | "stop_sequence")
            );
            if let Some(model) = message.get("model").and_then(Value::as_str)
                && !model.starts_with('<')
            {
                self.t.model = Some(model.to_string());
            }
            if let Some(effort) = rec.get("effort").and_then(Value::as_str) {
                self.t.effort = Some(effort.to_string());
            }
            let id = message
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let (Some(id), Some(usage)) = (&id, message.get("usage")) {
                let model = message.get("model").and_then(Value::as_str);
                let usage = pricing::ClaudeUsage::deserialize(usage).unwrap_or_default();
                self.usage_by_message
                    .insert(id.clone(), pricing::claude_cost(model, &usage));
            }
            for block in message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                match block.get("type").and_then(Value::as_str) {
                    Some("thinking") => self.t.last_block = LastBlock::Thinking,
                    Some("text") => {
                        self.t.last_block = LastBlock::Text;
                        let text = block
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if self.in_human_turn {
                            // One record per content block; blocks of one API message share
                            // its id, so the reply is the text of the turn's last message.
                            if id != self.reply_message {
                                self.reply_message.clone_from(&id);
                                self.t.reply_to_human.clear();
                            }
                            if !self.t.reply_to_human.is_empty() {
                                self.t.reply_to_human.push('\n');
                            }
                            self.t.reply_to_human.push_str(text);
                        }
                    }
                    Some("tool_use") => {
                        self.t.last_block = LastBlock::ToolUse;
                        let tool_id = block.get("id").and_then(Value::as_str).unwrap_or_default();
                        self.open.insert(tool_id.to_string());
                        match block.get("name").and_then(Value::as_str) {
                            Some("AskUserQuestion") => {
                                if let Some(input) = block.get("input") {
                                    self.t.question = ask_user_question(input, tool_id, &at);
                                }
                            }
                            Some("StructuredOutput") => {
                                self.structured_output = Some(tool_id.to_string());
                            }
                            Some("Monitor") => {
                                let field = |name: &str| {
                                    block
                                        .get("input")
                                        .and_then(|i| i.get(name))
                                        .and_then(Value::as_str)
                                        .map(str::to_string)
                                };
                                if let (Some(command), Some(description)) =
                                    (field("command"), field("description"))
                                {
                                    self.t.monitors.push(MonitorCall {
                                        command,
                                        description,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        } else {
            let blocks: &[Value] = match message.get("content") {
                Some(Value::Array(b)) => b,
                _ => &[],
            };
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("tool_result")
                    && let Some(id) = block.get("tool_use_id").and_then(Value::as_str)
                {
                    self.open.remove(id);
                    self.t.finished = self.structured_output.as_deref() == Some(id);
                    if self
                        .t
                        .question
                        .as_ref()
                        .is_some_and(|q| q.tool_use_id == id)
                    {
                        self.t.question = None;
                    }
                }
            }
            let is_prompt = matches!(message.get("content"), Some(Value::String(_)))
                || blocks
                    .iter()
                    .any(|b| b.get("type").and_then(Value::as_str) == Some("text"));
            let is_meta = rec.get("isMeta").and_then(Value::as_bool) == Some(true);
            if is_prompt && !is_meta {
                // A prompt starts a new turn. Only the human's turns carry questions for them.
                let origin = rec
                    .get("origin")
                    .and_then(|o| o.get("kind"))
                    .and_then(Value::as_str);
                self.in_human_turn = matches!(origin, None | Some("human"));
                self.t.finished = false;
                if self.in_human_turn {
                    self.t.human_spoke_last = true;
                    self.t.reply_to_human.clear();
                    self.reply_message = None;
                }
            }
        }
    }

    /// Snapshot the transcript as it stands after the lines fed so far.
    fn finish(&self) -> Transcript {
        let mut t = self.t.clone();
        t.spend = self.usage_by_message.values().copied().sum();
        t.open_tools = self.open.len();
        t
    }
}

/// Reduce a transcript to its current facts. Sidechain (sub-agent) records are
/// ignored: they describe helpers, not the session itself. This is a full
/// parse from byte zero; `ClaudeSource` uses `ParserState` directly to resume
/// from where a previous scan left off.
pub fn parse_transcript(reader: impl BufRead) -> Transcript {
    let mut state = ParserState::new();
    state.feed_lines(reader, 0, false);
    state.finish()
}

/// Turn a transcript plus liveness into the state rules' inputs.
#[must_use]
pub fn signals(t: &Transcript, live: Option<&LiveEntry>) -> Signals {
    Signals {
        live: live.is_some(),
        busy: live.is_some_and(|l| l.busy),
        question_waiting: t.question.is_some() || live.is_some_and(|l| l.waiting),
        pending_tool: t.open_tools > 0,
        human_spoke_last: t.human_spoke_last,
        api_error: t.api_error,
        last_block: t.last_block,
        reply_to_human: t.reply_to_human.clone(),
    }
}

/// One Claude Code session as discovered on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub session_id: String,
    pub transcript: Transcript,
    pub live: Option<LiveEntry>,
    pub helpers: Vec<Helper>,
    /// Where each listed helper works now, by helper id.
    pub helper_places: BTreeMap<String, super::Location>,
}

/// A working sub-agent, and where its latest folder sits.
fn helper_at(id: &str, t: &Transcript) -> (Helper, Option<super::Location>) {
    let place = t
        .latest_cwd
        .as_deref()
        .map(|cwd| super::locate(Path::new(cwd)));
    let in_tree = place.as_ref().filter(|p| p.worktree);
    let helper = Helper {
        id: id.to_string(),
        state: helper_state(t),
        worktree: in_tree.is_some(),
        branch: in_tree.and_then(|p| super::branch_of(&p.checkout)),
    };
    (helper, place)
}

/// A sub-agent silent this long was killed or lost, not left working: tool
/// calls time out well before it (Bash allows at most 10 minutes).
const HELPER_SILENCE: Duration = Duration::from_mins(10);

/// Whether `path` was written within `max` of `now`. A file written after `now`
/// (during the scan that took it) is as fresh as can be.
fn written_within(path: &Path, now: SystemTime, max: Duration) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|m| now.duration_since(m).unwrap_or(Duration::ZERO) <= max)
}

fn helper_state(t: &Transcript) -> SessionState {
    if t.api_error {
        SessionState::Error
    } else if t.last_block == LastBlock::Thinking {
        SessionState::Thinking
    } else {
        SessionState::Running
    }
}

/// A cached parser state for one transcript, plus enough to tell whether the
/// file on disk is still the same one grown in place.
struct CacheEntry {
    state: ParserState,
    /// Byte offset just past the last complete line consumed.
    offset: u64,
    /// The last (up to) `TAIL_WINDOW` bytes ending at `offset`, as last seen.
    /// Compared against the same window on the next scan to detect an
    /// in-place rewrite of already-consumed bytes (see `transcript`).
    tail: Vec<u8>,
    mtime: SystemTime,
    len: u64,
    dev: u64,
    ino: u64,
}

/// How many bytes just before `offset` we keep around to detect an in-place
/// rewrite of already-consumed content. Bounded and independent of file size,
/// which is the point: re-checking it costs one small seek + read, not a
/// re-parse of everything before `offset`.
const TAIL_WINDOW: u64 = 4096;

/// Read the `TAIL_WINDOW` bytes ending at `offset` (or all of them, if
/// `offset` is smaller than that). Used both to snapshot a cache entry's tail
/// and, on the next scan, to re-check it before trusting a resume.
fn tail_of(file: &mut fs::File, offset: u64) -> Option<Vec<u8>> {
    let window_start = offset.saturating_sub(TAIL_WINDOW);
    let len = usize::try_from(offset - window_start).unwrap_or(0);
    if len == 0 {
        return Some(Vec::new());
    }
    file.seek(SeekFrom::Start(window_start)).ok()?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// [`tail_of`] the file at `path`.
fn read_tail(path: &Path, offset: u64) -> Option<Vec<u8>> {
    tail_of(&mut fs::File::open(path).ok()?, offset)
}

/// Whether `file`, now `len` bytes long, still holds `tail` just before
/// `offset`: it only grew since it was read up to there.
fn grew_in_place(file: &mut fs::File, len: u64, offset: u64, tail: &[u8]) -> bool {
    len >= offset && tail_of(file, offset).as_deref() == Some(tail)
}

/// Scans Claude Code's files, re-reading a transcript only when it changed,
/// and reading only the bytes appended since the last scan when it grew.
pub struct ClaudeSource {
    home: PathBuf,
    /// Finished sessions stay visible this long after their last write.
    keep_finished: Duration,
    cache: HashMap<PathBuf, CacheEntry>,
}

impl ClaudeSource {
    #[must_use]
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            keep_finished: Duration::from_hours(12),
            cache: HashMap::new(),
        }
    }

    fn full_parse(
        &mut self,
        path: &Path,
        mtime: SystemTime,
        len: u64,
        dev: u64,
        ino: u64,
    ) -> Option<Transcript> {
        let file = fs::File::open(path).ok()?;
        let mut state = ParserState::new();
        state.keep_sidechains = path.components().any(|c| c.as_os_str() == "subagents");
        let offset = state.feed_lines(BufReader::new(file), 0, true);
        let transcript = state.finish();
        let tail = read_tail(path, offset).unwrap_or_default();
        self.cache.insert(
            path.to_path_buf(),
            CacheEntry {
                state,
                offset,
                tail,
                mtime,
                len,
                dev,
                ino,
            },
        );
        Some(transcript)
    }

    fn transcript(&mut self, path: &Path) -> Option<Transcript> {
        let meta = fs::metadata(path).ok()?;
        let mtime = meta.modified().ok()?;
        let len = meta.len();
        let dev = meta.dev();
        let ino = meta.ino();

        if let Some(entry) = self.cache.remove(path) {
            if entry.mtime == mtime && entry.len == len && entry.dev == dev && entry.ino == ino {
                let transcript = entry.state.finish();
                self.cache.insert(path.to_path_buf(), entry);
                return Some(transcript);
            }
            // Same (dev, ino) and grown is not enough on its own: Claude Code
            // 2.1.284's `performRemoveByUuid` opens the transcript `r+`,
            // truncates it at a removed line's start and rewrites the tail —
            // same inode, file shrinks, and if the session then appends
            // before our next scan the file can land back at or above the
            // old offset with entirely different bytes there. A plain
            // truncate-and-rewrite (`fs::write`/`writeFileSync`) does the
            // same on some platforms. So before resuming from `offset` we
            // also re-check the `TAIL_WINDOW` bytes just before it against
            // what we saw there last time: one bounded seek + read,
            // independent of file size, which is what keeps the incremental
            // path worth having. A genuinely shorter file, a different
            // (dev, ino) (replaced via rename), or a tail mismatch (rewritten
            // in place) all fall through to a full re-parse. Residual blind
            // spot: an edit that rewrites everything before `offset` but
            // happens to leave those exact last TAIL_WINDOW bytes
            // byte-for-byte identical would still be missed — not something
            // Claude Code's own writers do, but a contrived adversarial edit
            // could construct it.
            if entry.dev == dev
                && entry.ino == ino
                && let Ok(mut file) = fs::File::open(path)
                && grew_in_place(&mut file, len, entry.offset, &entry.tail)
                && file.seek(SeekFrom::Start(entry.offset)).is_ok()
            {
                let mut state = entry.state;
                let offset = state.feed_lines(BufReader::new(file), entry.offset, true);
                let transcript = state.finish();
                let tail = read_tail(path, offset).unwrap_or_default();
                self.cache.insert(
                    path.to_path_buf(),
                    CacheEntry {
                        state,
                        offset,
                        tail,
                        mtime,
                        len,
                        dev,
                        ino,
                    },
                );
                return Some(transcript);
            }
        }
        self.full_parse(path, mtime, len, dev, ino)
    }

    /// Every live session, plus finished ones written to recently.
    pub fn scan(&mut self, now: SystemTime) -> Vec<Found> {
        let claude = self.home.join(".claude");
        let live: HashMap<String, LiveEntry> = read_registry(&claude.join("sessions"), pid_alive)
            .into_iter()
            .map(|e| (e.session_id.clone(), e))
            .collect();

        let mut found = Vec::new();
        let Ok(dirs) = fs::read_dir(claude.join("projects")) else {
            return found;
        };
        for dir in dirs
            .filter_map(Result::ok)
            .map(|d| d.path())
            .filter(|p| p.is_dir())
        {
            let Ok(files) = fs::read_dir(&dir) else {
                continue;
            };
            for path in files.filter_map(Result::ok).map(|f| f.path()) {
                if path.extension().is_none_or(|x| x != "jsonl") {
                    continue;
                }
                let Some(session_id) = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
                else {
                    continue;
                };
                let entry = live.get(&session_id).cloned();
                if entry.is_none() && !written_within(&path, now, self.keep_finished) {
                    continue;
                }
                if let Some(mut transcript) = self.transcript(&path) {
                    // Sub-agents write `<session>/subagents/**/*.jsonl`; what they spend is the session's.
                    let mut paths = Vec::new();
                    collect_jsonl(&dir.join(&session_id).join("subagents"), &mut paths);
                    let mut helpers = Vec::new();
                    let mut helper_places = BTreeMap::new();
                    // Only `agent-*.jsonl` is a sub-agent: a workflow keeps its `journal.jsonl` beside them.
                    for path in paths.into_iter().filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with("agent-"))
                    }) {
                        let Some(t) = self.transcript(&path) else {
                            continue;
                        };
                        transcript.spend = transcript.spend + t.spend;
                        if !t.finished
                            && written_within(&path, now, HELPER_SILENCE)
                            && let Some(id) = path.file_stem().and_then(|s| s.to_str())
                        {
                            let (helper, place) = helper_at(id, &t);
                            helpers.push(helper);
                            if let Some(place) = place {
                                helper_places.insert(id.to_string(), place);
                            }
                        }
                    }
                    // Stable desk order, whatever order the directory lists in.
                    helpers.sort_by(|a, b| a.id.cmp(&b.id));
                    found.push(Found {
                        session_id,
                        transcript,
                        live: entry,
                        helpers,
                        helper_places,
                    });
                }
            }
        }
        for pid in unregistered_pids(&claude.join("sessions"), now, pid_alive) {
            let Some(cwd) = super::codex::cwd_of(pid) else {
                continue;
            };
            let session_id = unregistered_id(pid);
            let mut transcript = ParserState::new().t;
            transcript.cwd = Some(cwd.clone());
            transcript.title = Some("New Claude Code session".to_string());
            found.push(Found {
                live: Some(LiveEntry {
                    pid,
                    session_id: session_id.clone(),
                    cwd,
                    busy: false,
                    waiting: false,
                    name: None,
                }),
                session_id,
                transcript,
                helpers: Vec::new(),
                helper_places: BTreeMap::new(),
            });
        }
        found
    }

    /// Pid of a running session, for focusing its terminal.
    pub fn pid_of(&self, session_id: &str) -> Option<i32> {
        read_registry(&self.home.join(".claude").join("sessions"), pid_alive)
            .into_iter()
            .find(|e| e.session_id == session_id)
            .map(|e| e.pid)
            .or_else(|| {
                let pid = session_id.strip_prefix(UNREGISTERED_PREFIX)?.parse().ok()?;
                pid_alive(pid).then_some(pid)
            })
    }
}

pub mod chat;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod verifier_attacks;
