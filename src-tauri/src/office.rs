//! The office: live discovery plus the app's own records, behind one lock.
//! Every Tauri command goes through here, so there is one source of truth.

use crate::discovery::{self, Discovered, Discovery};
use crate::focus;
use crate::model::{
    AgentMessage, AnswerVia, ControlMode, HireFailed, HireRequest, OfficeSnapshot, Party, Project,
    QuestionAnswer, Session, SessionState, Spend, TerminalBacklog, TerminalExit, TodoItem,
    ToolKind, ToolOptions,
};
use crate::options;
use crate::project_files::{self, ProjectFiles};
use crate::pty::{self, Terminals};
use crate::store::Store;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

struct Inner {
    discovery: Discovery,
    found: Discovered,
    /// Decisions and TODOs from each floor's own files, by project id.
    files: BTreeMap<String, ProjectFiles>,
    store: Store,
    /// The session each app-owned terminal is running, by terminal id. It
    /// outlives the agent so its last output stays readable.
    owned: BTreeMap<String, String>,
    /// Every session a terminal has run, by session id, so a panel opened on an
    /// earlier session (before a `/clear`) keeps reaching the same terminal.
    ran_in: BTreeMap<String, String>,
    /// Hired terminals no session has been bound to yet, with the task they
    /// were hired for: if one exits first, the hire failed after all.
    hired: BTreeMap<String, String>,
}

pub struct Office {
    home: PathBuf,
    inner: Mutex<Inner>,
    terminals: Terminals,
}

/// Pushed when a hired agent exits before it ever reached a desk.
pub const HIRE_FAILED_EVENT: &str = "office://hire-failed";

/// How far up a live session's process tree to look for a terminal we started
/// (a launcher can sit in between; Codex and Claude Code themselves do not).
const MAX_ANCESTRY: usize = 4;

/// How long a hired agent has to keep running, once the login shell has handed
/// over to it, before the hire counts as started.
const LAUNCH_GRACE: Duration = Duration::from_millis(1500);
/// The longest a login shell may take to hand over (a slow profile). Past it the
/// hire counts as started, and a later exit is reported by [`Office::hire_failed`].
const LAUNCH_LIMIT: Duration = Duration::from_secs(15);
const LAUNCH_POLL: Duration = Duration::from_millis(100);
/// Process names of shells: while the started process still runs one, the
/// login shell has not handed over to the agent yet.
const SHELL_NAMES: &[&str] = &["zsh", "bash", "sh", "fish", "dash", "ksh", "tcsh", "csh"];
/// `claude` subcommands (from `claude --help`, 2.1.285, plus commander's own
/// `help`). A task that is exactly one of these runs the subcommand even after
/// `--`; codex takes such a task as its prompt.
const CLAUDE_SUBCOMMANDS: &[&str] = &[
    "agents",
    "attach",
    "auth",
    "auto-mode",
    "doctor",
    "gateway",
    "help",
    "import",
    "install",
    "logs",
    "mcp",
    "plugin",
    "plugins",
    "project",
    "respawn",
    "rm",
    "setup-token",
    "stop",
    "kill",
    "ultrareview",
    "update",
    "upgrade",
];

/// The app-owned terminal a process runs in, and how many generations below
/// the agent we started there the process is (0: it is that agent).
fn owning_terminal(
    pid: i32,
    children: &BTreeMap<i32, String>,
    parent_of: impl Fn(i32) -> Option<i32>,
) -> Option<(String, usize)> {
    let mut current = pid;
    for depth in 0..=MAX_ANCESTRY {
        if let Some(id) = children.get(&current) {
            return Some((id.clone(), depth));
        }
        match parent_of(current) {
            Some(parent) if parent > 1 => current = parent,
            _ => return None,
        }
    }
    None
}

/// Bind live sessions to the terminals we started them in, then give every
/// bound session full control. Returns the sessions now live in some other
/// terminal.
///
/// A terminal goes to the live session closest to the agent we started (a
/// session the agent itself launched does not take it), then to the one most
/// recently active (a new session id in the same process, as after `/clear`).
/// A session that is live anywhere else is never ours, whatever it ran in
/// before. A session whose pid cannot be read right now keeps what it had.
fn claim_owned(
    sessions: &mut [Session],
    owned: &mut BTreeMap<String, String>,
    children: &BTreeMap<i32, String>,
    pid_of: impl Fn(&str) -> Option<i32>,
    parent_of: impl Fn(i32) -> Option<i32>,
) -> Vec<String> {
    let mut best: BTreeMap<String, (usize, &Session)> = BTreeMap::new();
    let mut elsewhere: Vec<String> = Vec::new();
    for s in sessions
        .iter()
        .filter(|s| s.control == ControlMode::RaiseWindow)
    {
        // Neither ours now nor bound before: nothing to decide, so skip the lookup.
        if children.is_empty() && !owned.values().any(|id| id == &s.id) {
            continue;
        }
        let Some(pid) = pid_of(&s.id) else { continue };
        let Some((terminal, depth)) = owning_terminal(pid, children, &parent_of) else {
            elsewhere.push(s.id.clone());
            continue;
        };
        let better = best.get(&terminal).is_none_or(|(d, b)| {
            depth < *d || (depth == *d && s.last_activity_at > b.last_activity_at)
        });
        if better {
            best.insert(terminal, (depth, s));
        }
    }
    for (terminal, (_, s)) in best {
        owned.insert(terminal, s.id.clone());
    }
    owned.retain(|_, s| !elsewhere.contains(s));
    for s in sessions.iter_mut() {
        if owned.values().any(|id| id == &s.id) {
            s.control = ControlMode::Full;
        }
    }
    elsewhere
}

/// A finished session a terminal ran earlier (before a `/clear`) opens that
/// terminal too, rather than claiming it was started outside the app.
fn reopen_earlier_sessions(sessions: &mut [Session], ran_in: &BTreeMap<String, String>) {
    for s in sessions
        .iter_mut()
        .filter(|s| s.control == ControlMode::ReadOnly && ran_in.contains_key(&s.id))
    {
        s.control = ControlMode::Full;
    }
}

/// The last `n` non-blank lines a terminal printed, as plain text.
fn last_lines(terminals: &Terminals, terminal: &str, n: usize) -> Result<Vec<String>, String> {
    let output = pty::plain_text(&terminals.backlog(terminal)?.data);
    let said: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    Ok(said[said.len().saturating_sub(n)..].to_vec())
}

fn launch_failure(
    terminals: &Terminals,
    terminal: &str,
    code: Option<u32>,
) -> Result<String, String> {
    let tail = last_lines(terminals, terminal, 4)?.join(" ");
    let code = code.map_or_else(String::new, |c| format!(" (exit code {c})"));
    Ok(if tail.is_empty() {
        format!("the agent stopped as soon as it started{code}")
    } else {
        format!("the agent stopped as soon as it started{code}: {tail}")
    })
}

/// Start `command` in a new terminal in `root`. A launch that fails (the tool
/// not on the PATH, a broken shell profile, a rejected option) exits before
/// any desk could show its terminal, so that becomes the error here. The agent
/// has to outlast [`LAUNCH_GRACE`] from the moment the login shell hands over
/// to it, however long the shell's profile takes (up to [`LAUNCH_LIMIT`]).
fn start_agent(terminals: &Terminals, root: &Path, command: &str) -> Result<String, String> {
    let terminal = terminals.spawn_shell(root, command)?;
    let pid = terminals.pid(&terminal)?;
    let started = Instant::now();
    let mut handed_over: Option<Instant> = None;
    loop {
        if let Some(code) = terminals.wait_exit(&terminal, LAUNCH_POLL)? {
            return Err(launch_failure(terminals, &terminal, code)?);
        }
        let now = Instant::now();
        match handed_over {
            Some(at) if now.duration_since(at) >= LAUNCH_GRACE => return Ok(terminal),
            None if focus::process_name(pid)
                .is_some_and(|name| !SHELL_NAMES.contains(&name.as_str())) =>
            {
                handed_over = Some(now);
            }
            None if now.duration_since(started) >= LAUNCH_LIMIT => return Ok(terminal),
            Some(_) | None => {}
        }
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Office {
    pub fn new(home: PathBuf, store: Store, terminals: Terminals) -> Self {
        let discovery = Discovery::new(home.clone());
        Self {
            home,
            inner: Mutex::new(Inner {
                discovery,
                found: Discovered::default(),
                files: BTreeMap::new(),
                store,
                owned: BTreeMap::new(),
                ran_in: BTreeMap::new(),
                hired: BTreeMap::new(),
            }),
            terminals,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind; keep serving it.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Re-scan the tools' files. Returns whether anything visible changed.
    pub fn refresh(&self) -> bool {
        let mut guard = self.lock();
        let inner = &mut *guard;
        let mut found = inner.discovery.scan(SystemTime::now());
        let pinned = inner.store.records.pinned_floors.clone();
        add_pinned_floors(&mut found, &pinned, &self.home);
        let discovery = &inner.discovery;
        let elsewhere = claim_owned(
            &mut found.sessions,
            &mut inner.owned,
            &self.terminals.running_children(),
            |id| discovery.pid_of(id),
            focus::parent_pid,
        );
        for (terminal, session) in &inner.owned {
            inner.ran_in.insert(session.clone(), terminal.clone());
        }
        inner
            .ran_in
            .retain(|session, _| !elsewhere.contains(session));
        inner
            .hired
            .retain(|terminal, _| !inner.owned.contains_key(terminal));
        reopen_earlier_sessions(&mut found.sessions, &inner.ran_in);
        let files: BTreeMap<String, ProjectFiles> = found
            .roots
            .iter()
            .map(|(id, root)| (id.clone(), project_files::read(root, id)))
            .collect();
        let changed = found != inner.found || files != inner.files;
        inner.found = found;
        inner.files = files;
        changed
    }

    /// A floor's TODOs for one assignee: its file when it has one, else the app's store.
    fn todos_for(inner: &Inner, project_id: &str, assignee: Party) -> Vec<TodoItem> {
        let from_file = inner.files.get(project_id).and_then(|f| match assignee {
            Party::Human => f.human_todos.clone(),
            Party::Ai => f.agent_todos.clone(),
        });
        from_file.unwrap_or_else(|| {
            inner
                .store
                .records
                .todos
                .iter()
                .filter(|t| t.project_id == project_id && t.assignee == assignee)
                .cloned()
                .collect()
        })
    }

    pub fn snapshot(&self) -> OfficeSnapshot {
        let inner = self.lock();
        let r = &inner.store.records;
        let mut questions = inner.found.questions.clone();
        questions.extend(inner.files.values().flat_map(|f| f.open.iter().cloned()));
        let mut decisions = r.decisions.clone();
        decisions.extend(
            inner
                .files
                .values()
                .flat_map(|f| f.decided.iter().flatten().cloned()),
        );
        let todos = inner
            .found
            .projects
            .iter()
            .flat_map(|p| {
                let mut all = Self::todos_for(&inner, &p.id, Party::Human);
                all.extend(Self::todos_for(&inner, &p.id, Party::Ai));
                all
            })
            .collect();
        OfficeSnapshot {
            projects: inner.found.projects.clone(),
            sessions: inner.found.sessions.clone(),
            questions,
            decisions,
            todos,
            messages: r.messages.clone(),
        }
    }

    pub fn hire_options(&self) -> Vec<ToolOptions> {
        options::hire_options(&self.home)
    }

    /// Start an agent in an in-app terminal, in the floor's folder. It appears
    /// as a desk as soon as the tool writes its session files.
    pub fn spawn_session(&self, project_id: &str, req: &HireRequest) -> Result<Session, String> {
        // The absolute root, not the `~/…` display path.
        let root = self
            .lock()
            .found
            .roots
            .get(project_id)
            .cloned()
            .ok_or_else(|| format!("unknown project: {project_id}"))?;
        let tool = self
            .hire_options()
            .into_iter()
            .find(|o| o.tool == req.tool)
            .ok_or_else(|| "that tool is not set up on this Mac".to_string())?;
        let model = tool
            .models
            .iter()
            .find(|m| m.id == req.model)
            .ok_or_else(|| format!("{} is not a model the app can start", req.model))?;
        if !model.efforts.iter().any(|e| e.id == req.effort) {
            return Err(format!(
                "{} does not support {} effort",
                model.label, req.effort
            ));
        }
        let task = req.title.trim();
        if req.tool == ToolKind::Codex && task.is_empty() {
            // Codex writes nothing discovery can see until its first turn.
            return Err(
                "Codex only shows up in the building once it starts work, so give it a task"
                    .to_string(),
            );
        }
        if req.tool == ToolKind::ClaudeCode && CLAUDE_SUBCOMMANDS.contains(&task) {
            return Err(format!(
                "“{task}” is also a claude command, so describe the task in a few more words"
            ));
        }
        let shell = pty::login_shell();
        let quote = |s: &str| pty::quote(&shell, s);
        // `--` ends the options, so a task starting with `-` stays the prompt.
        let prompt = if task.is_empty() {
            String::new()
        } else {
            format!(" -- {}", quote(task))
        };
        let command = match req.tool {
            ToolKind::ClaudeCode => format!(
                "claude --model {} --effort {}{prompt}",
                options::claude_alias(&model.id),
                req.effort
            ),
            ToolKind::Codex => format!(
                "codex -m {} -c {}{prompt}",
                quote(&model.id),
                quote(&format!("model_reasoning_effort=\"{}\"", req.effort))
            ),
            _ => return Err("only Claude Code and Codex can be hired so far".to_string()),
        };
        let terminal = start_agent(&self.terminals, &root, &command)?;
        {
            // Under the lock `hire_failed` takes: an exit either lands here or finds the entry.
            let mut inner = self.lock();
            if let Some(code) = self.terminals.wait_exit(&terminal, Duration::ZERO)? {
                return Err(launch_failure(&self.terminals, &terminal, code)?);
            }
            inner.hired.insert(
                terminal,
                if task.is_empty() {
                    "New task".to_string()
                } else {
                    task.to_string()
                },
            );
        }
        // The real session id arrives with discovery; this is only the receipt.
        Ok(Session {
            id: format!("hired-{}", chrono::Utc::now().timestamp_millis()),
            tool: req.tool,
            project_id: project_id.to_string(),
            title: if task.is_empty() {
                "New task".to_string()
            } else {
                task.to_string()
            },
            state: SessionState::Thinking,
            control: ControlMode::Full,
            model: Some(model.id.clone()),
            effort: Some(req.effort.clone()),
            last_activity_at: now_iso(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers: Vec::new(),
        })
    }

    /// Add a floor for a repo folder the human picked. Idempotent: a folder that
    /// already has a floor returns that floor.
    pub fn add_floor(&self, path: &str) -> Result<Project, String> {
        let expanded = match path.trim().strip_prefix('~') {
            Some(rest) => self.home.join(rest.trim_start_matches('/')),
            None => PathBuf::from(path.trim()),
        };
        if !expanded.is_absolute() {
            return Err("give the full path to the folder, for example ~/code/app".to_string());
        }
        if !expanded.is_dir() {
            return Err(format!(
                "{} is not a folder on this Mac",
                expanded.display()
            ));
        }
        let root = discovery::project_root(&expanded);
        let key = root.to_string_lossy().into_owned();
        {
            let mut inner = self.lock();
            if !inner.store.records.pinned_floors.contains(&key) {
                inner.store.records.pinned_floors.push(key);
                inner.store.save()?;
            }
        }
        self.refresh();
        let id = discovery::project_id(&root);
        self.lock()
            .found
            .projects
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| "the floor could not be added".to_string())
    }

    /// Ask macOS for a folder with its own picker. None when the human cancels.
    pub fn pick_folder(&self) -> Result<Option<String>, String> {
        focus::choose_folder("Choose the repo folder for the new floor")
    }

    pub fn answer_question(
        &self,
        question_id: &str,
        answer: &QuestionAnswer,
    ) -> Result<(), String> {
        {
            let inner = self.lock();
            let question = inner
                .found
                .questions
                .iter()
                .chain(inner.files.values().flat_map(|f| f.open.iter()))
                .find(|q| q.id == question_id)
                .ok_or_else(|| format!("unknown question: {question_id}"))?;
            match question.answer_via {
                AnswerVia::Terminal => {
                    return Err(
                        "this agent was started outside the app; answer it in its own terminal"
                            .to_string(),
                    );
                }
                // App-owned terminals (PTY) arrive with the in-app terminal work.
                AnswerVia::App => {
                    return Err("answering in the app is not wired to a terminal yet".to_string());
                }
                AnswerVia::File => {
                    let text = match answer {
                        QuestionAnswer::Option { option_id } => question
                            .options
                            .iter()
                            .find(|o| &o.id == option_id)
                            .map(|o| o.label.clone())
                            .ok_or_else(|| format!("unknown option {option_id}"))?,
                        QuestionAnswer::Other { text } if !text.trim().is_empty() => {
                            text.trim().to_string()
                        }
                        QuestionAnswer::Other { .. } => {
                            return Err("empty free-text answer".to_string());
                        }
                    };
                    let (file, line) =
                        project_files::parse_item_id(question_id).ok_or("malformed question id")?;
                    let root = inner
                        .found
                        .roots
                        .get(&question.project_id)
                        .ok_or("that floor is no longer in the building")?;
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    project_files::decide(root, file, line, &text, &today)?;
                }
            }
        }
        self.refresh();
        Ok(())
    }

    pub fn tick_todo(&self, todo_id: &str) -> Result<(), String> {
        let from_file = {
            let inner = self.lock();
            inner
                .files
                .values()
                .flat_map(|f| f.human_todos.iter().chain(f.agent_todos.iter()).flatten())
                .find(|t| t.id == todo_id)
                .map(|t| {
                    (
                        t.project_id.clone(),
                        project_files::parse_item_id(todo_id)
                            .map(|(file, line)| (file.to_string(), line)),
                    )
                })
        };
        if let Some((project_id, location)) = from_file {
            let (file, line) = location.ok_or("malformed todo id")?;
            let root = self
                .lock()
                .found
                .roots
                .get(&project_id)
                .cloned()
                .ok_or("that floor is no longer in the building")?;
            project_files::tick(&root, &file, line)?;
            self.refresh();
            return Ok(());
        }
        let mut inner = self.lock();
        let todo = inner
            .store
            .records
            .todos
            .iter_mut()
            .find(|t| t.id == todo_id)
            .ok_or_else(|| format!("unknown todo: {todo_id}"))?;
        todo.done = true;
        inner.store.save()
    }

    /// Record a note between two agents (delivery into the recipient arrives with the MCP work).
    pub fn send_agent_message(&self, from: &str, to: &str, text: &str) -> Result<(), String> {
        let mut inner = self.lock();
        for id in [from, to] {
            if !inner.found.sessions.iter().any(|s| s.id == id) {
                return Err(format!("unknown session: {id}"));
            }
        }
        if from == to {
            return Err("an agent cannot message itself".to_string());
        }
        if text.trim().is_empty() {
            return Err("empty agent message".to_string());
        }
        let id = inner.store.next_id("m");
        inner.store.push_message(AgentMessage {
            id,
            from_session_id: from.to_string(),
            to_session_id: to.to_string(),
            text: text.trim().to_string(),
            sent_at: now_iso(),
        });
        inner.store.save()
    }

    /// A hired agent's terminal has exited: if no session was ever bound to it,
    /// the hire failed after it was reported as started, so say so.
    pub fn hire_failed(&self, exit: &TerminalExit) -> Option<HireFailed> {
        let title = self.lock().hired.remove(&exit.terminal_id)?;
        let last_lines = last_lines(&self.terminals, &exit.terminal_id, 4).unwrap_or_default();
        Some(HireFailed {
            title,
            exit_code: exit.exit_code,
            last_lines,
        })
    }

    /// The app-owned terminal a session runs in.
    fn terminal_of(&self, session_id: &str) -> Result<String, String> {
        let inner = self.lock();
        inner
            .owned
            .iter()
            .find(|(_, s)| s.as_str() == session_id)
            .map(|(t, _)| t)
            .or_else(|| inner.ran_in.get(session_id))
            .cloned()
            .ok_or_else(|| {
                format!("{session_id} was not started in the app, so it has no terminal here")
            })
    }

    pub fn terminal_backlog(&self, session_id: &str) -> Result<TerminalBacklog, String> {
        self.terminals.backlog(&self.terminal_of(session_id)?)
    }

    pub fn terminal_write(&self, session_id: &str, data: &str) -> Result<(), String> {
        self.terminals.write(&self.terminal_of(session_id)?, data)
    }

    pub fn terminal_resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        self.terminals
            .resize(&self.terminal_of(session_id)?, cols, rows)
    }

    pub fn focus_session(&self, session_id: &str) -> Result<(), String> {
        let pid = {
            let inner = self.lock();
            let session = inner
                .found
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .ok_or_else(|| format!("unknown session: {session_id}"))?;
            if session.control != ControlMode::RaiseWindow {
                return Err(format!(
                    "{} is not running in a terminal window",
                    session.title
                ));
            }
            inner
                .discovery
                .pid_of(session_id)
                .ok_or("its process has exited")?
        };
        focus::focus_pid(pid)
    }
}

/// Floors the human pinned that no agent is working on yet: added with no desks.
fn add_pinned_floors(found: &mut Discovered, pinned: &[String], home: &Path) {
    for key in pinned {
        let root = PathBuf::from(key);
        let id = discovery::project_id(&root);
        if found.roots.contains_key(&id) || !root.is_dir() {
            continue;
        }
        found.projects.push(Project {
            id: id.clone(),
            name: root
                .file_name()
                .map_or_else(|| key.clone(), |n| n.to_string_lossy().into_owned()),
            path: discovery::display_path(&root, home),
            session_ids: Vec::new(),
        });
        found.roots.insert(id, root);
    }
}

#[cfg(test)]
mod tests;
