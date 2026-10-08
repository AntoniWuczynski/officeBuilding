//! Launching an agent in an in-app terminal: the command each tool is started
//! with, and the desk a tool discovery cannot see gets from its terminal.

use crate::model::{
    ControlMode, HireRequest, ModelOption, Session, SessionState, Spend, ToolKind, ToolOptions,
};
use crate::pty;
use crate::tools;
use std::path::PathBuf;
use std::time::Duration;

/// What a terminal-backed desk shows, fixed when the agent was hired.
pub(super) struct Desk {
    pub(super) tool: ToolKind,
    pub(super) project_id: String,
    pub(super) root: PathBuf,
    pub(super) title: String,
    pub(super) model: String,
    pub(super) effort: String,
}

/// How the terminal a launch started is accounted for.
pub(super) enum Launched {
    /// A tool discovery sees: its session binds to the terminal once it writes files.
    Discovered { title: String },
    /// A tool discovery cannot see: the terminal itself is the session.
    Desk(Desk),
}

/// A terminal-backed desk counts as running while its terminal printed this recently.
pub(super) const DESK_ACTIVE: Duration = Duration::from_secs(10);
/// The effort id of a model with no effort choices: pass no effort flag.
const DEFAULT_EFFORT: &str = "default";
/// The model id that means "the tool's own default": pass no model flag.
const DEFAULT_MODEL: &str = "default";

fn tool_label(tool: ToolKind) -> &'static str {
    match tool {
        ToolKind::ClaudeCode => "Claude Code",
        ToolKind::Codex => "Codex",
        ToolKind::Antigravity => "Antigravity",
        ToolKind::Opencode => "opencode",
        ToolKind::Cursor => "Cursor",
        ToolKind::Unknown => "That tool",
    }
}

/// The options of `tool` when it can be hired: set up here and installed.
pub(super) fn hireable(options: Vec<ToolOptions>, tool: ToolKind) -> Result<ToolOptions, String> {
    let found = options
        .into_iter()
        .find(|o| o.tool == tool)
        .ok_or_else(|| "that tool is not set up on this Mac".to_string())?;
    if found.available {
        Ok(found)
    } else {
        Err(format!("{} is not installed on this Mac", tool_label(tool)))
    }
}

/// Only Claude Code and Codex write session files discovery can read.
pub(super) fn is_discovered(tool: ToolKind) -> bool {
    matches!(tool, ToolKind::ClaudeCode | ToolKind::Codex)
}

/// The session id of a terminal-backed desk: the terminal panel attaches by it.
pub(super) fn desk_session_id(terminal: &str) -> String {
    format!("terminal-{terminal}")
}

pub(super) fn desk_session(terminal: &str, desk: &Desk, idle: Duration) -> Session {
    let last = chrono::Duration::from_std(idle)
        .ok()
        .and_then(|d| chrono::Utc::now().checked_sub_signed(d))
        .unwrap_or_else(chrono::Utc::now);
    Session {
        id: desk_session_id(terminal),
        tool: desk.tool,
        project_id: desk.project_id.clone(),
        title: desk.title.clone(),
        state: if idle < DESK_ACTIVE {
            SessionState::Running
        } else {
            SessionState::Idle
        },
        control: ControlMode::Full,
        model: Some(desk.model.clone()),
        effort: Some(desk.effort.clone()),
        last_activity_at: last.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        pending_question_ids: Vec::new(),
        spend: Spend::default(),
        helpers: Vec::new(),
        worktree: false,
    }
}

/// The shell command that starts `model` of `req.tool` on `task` (empty: no
/// task). Flags are from each tool's own `--help` or docs: `claude --help`
/// (`--model`, `--effort`, `--settings <file-or-json>`), `codex`, `agy --help`
/// (`--model`, `-i`), opencode's CLI docs (`--model`) and Cursor's CLI docs
/// (`--model`). The last two take no prompt: their interactive-with-prompt
/// form is unconfirmed, so the task only names the desk.
pub(super) fn launch_command(
    req: &HireRequest,
    model: &ModelOption,
    task: &str,
    shell: &str,
) -> Result<String, String> {
    let quote = |s: &str| pty::quote(shell, s);
    let bare_effort = req.effort == DEFAULT_EFFORT;
    // `--` ends the options, so a task starting with `-` stays the prompt.
    let prompt = if task.is_empty() {
        String::new()
    } else {
        format!(" -- {}", quote(task))
    };
    let binary = tools::binary(req.tool).ok_or("that tool cannot be hired")?;
    Ok(match req.tool {
        ToolKind::ClaudeCode => {
            let effort = if bare_effort {
                String::new()
            } else {
                format!(" --effort {}", quote(&req.effort))
            };
            format!(
                "{binary} --model {}{effort} --settings {}{prompt}",
                quote(&model.id),
                quote(&format!("{{\"ultracode\":{}}}", req.ultracode))
            )
        }
        ToolKind::Codex => {
            let model_flag = if model.id == DEFAULT_MODEL {
                String::new()
            } else {
                format!(" -m {}", quote(&model.id))
            };
            let effort = if bare_effort {
                String::new()
            } else {
                format!(
                    " -c {}",
                    quote(&format!("model_reasoning_effort=\"{}\"", req.effort))
                )
            };
            format!("{binary}{model_flag}{effort}{prompt}")
        }
        ToolKind::Antigravity => {
            let name = if bare_effort {
                model.id.clone()
            } else {
                let label = model
                    .efforts
                    .iter()
                    .find(|e| e.id == req.effort)
                    .map_or(req.effort.as_str(), |e| e.label.as_str());
                format!("{} ({label})", model.id)
            };
            // `-i=` because `agy` reads a dash-led word after a bare `-i` as a flag.
            let prompt = if task.is_empty() {
                String::new()
            } else {
                format!(" -i={}", quote(task))
            };
            format!("{binary} --model {}{prompt}", quote(&name))
        }
        ToolKind::Opencode | ToolKind::Cursor if model.id == DEFAULT_MODEL => binary.to_string(),
        ToolKind::Opencode | ToolKind::Cursor => {
            format!("{binary} --model {}", quote(&model.id))
        }
        ToolKind::Unknown => return Err("that tool cannot be hired".to_string()),
    })
}
