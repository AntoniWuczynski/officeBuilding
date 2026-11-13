//! Launching an agent in an in-app terminal: the command each tool is started
//! with, and the desk a tool discovery cannot see gets from its terminal.

use crate::model::{
    ControlMode, HireRequest, ModelOption, Session, SessionState, Spend, ToolKind, ToolOptions,
};
use crate::pty;
use crate::setup::Launch;
use crate::tools;
use std::path::PathBuf;
use std::time::Duration;

/// `claude` subcommands (from `claude --help`, 2.1.285, plus commander's own
/// `help`). A task that is exactly one of these runs the subcommand even after
/// `--`; codex takes such a task as its prompt.
pub(super) const CLAUDE_SUBCOMMANDS: &[&str] = &[
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
pub(super) const DEFAULT_EFFORT: &str = "default";
/// The model id that means "the tool's own default": pass no model flag.
pub(super) const DEFAULT_MODEL: &str = "default";
/// Codex otherwise opens on an "Update available!" menu whenever a newer
/// release is out, and the agent never starts work (`check_for_update_on_startup`
/// in Codex's config).
pub(super) const CODEX_NO_UPDATE_CHECK: &str = "-c check_for_update_on_startup=false";

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

/// The model of `tool` named `model` (None: its default), and the effort to run it at
/// (None: the model's default), checked against what the tool offers.
pub(super) fn chosen_model<'a>(
    tool: &'a ToolOptions,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<(&'a ModelOption, String), String> {
    let id = model.unwrap_or(&tool.default_model);
    let found = tool
        .models
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| format!("{id} is not a model the app can start"))?;
    let effort = effort.unwrap_or(&found.default_effort);
    if !found.efforts.iter().any(|e| e.id == effort) {
        return Err(format!("{} does not support {effort} effort", found.label));
    }
    Ok((found, effort.to_string()))
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
        pets: Vec::new(),
        worktree: false,
        branch: None,
    }
}

/// The shell command that starts `model` of `req.tool` on `task` (empty: no
/// task). Flags are from each tool's own `--help` or docs: `claude --help`
/// (`--model`, `--effort`, `--settings <file-or-json>`), `codex`, `agy --help`
/// (`--model`, `-i`), opencode's CLI docs (`--model`) and Cursor's CLI docs
/// (`--model`). The last two take no prompt: their interactive-with-prompt
/// form is unconfirmed, so the task only names the desk. `setup`'s environment
/// goes before the binary, its options after it.
pub(super) fn launch_command(
    req: &HireRequest,
    model: &ModelOption,
    task: &str,
    shell: &str,
    setup: &Launch,
) -> Result<String, String> {
    let quote = |s: &str| pty::quote(shell, s);
    let env = setup.prefix(&quote);
    let refuse_peer_messages = setup.refuse_peer_messages;
    let setup = setup.args(&quote);
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
            // One `--settings` only, so a fixed role's refusal shares the JSON
            // (<https://code.claude.com/docs/en/settings-reference>, `crossSessionInbound`).
            let refuse = if refuse_peer_messages {
                ",\"crossSessionInbound\":\"refuse\""
            } else {
                ""
            };
            format!(
                "{env}{binary}{setup} --model {}{effort} --settings {}{prompt}",
                quote(&model.id),
                quote(&format!("{{\"ultracode\":{}{refuse}}}", req.ultracode))
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
            format!("{env}{binary}{setup} {CODEX_NO_UPDATE_CHECK}{model_flag}{effort}{prompt}")
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
            format!("{env}{binary}{setup} --model {}{prompt}", quote(&name))
        }
        ToolKind::Opencode | ToolKind::Cursor if model.id == DEFAULT_MODEL => {
            format!("{env}{binary}{setup}")
        }
        ToolKind::Opencode | ToolKind::Cursor => {
            format!("{env}{binary}{setup} --model {}", quote(&model.id))
        }
        ToolKind::Unknown => return Err("that tool cannot be hired".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EffortOption;

    fn options() -> ToolOptions {
        let effort = |id: &str| EffortOption {
            id: id.to_string(),
            label: id.to_string(),
            description: String::new(),
        };
        let model = |id: &str, label: &str, efforts: &[&str], default: &str| ModelOption {
            id: id.to_string(),
            label: label.to_string(),
            efforts: efforts.iter().map(|e| effort(e)).collect(),
            default_effort: default.to_string(),
        };
        ToolOptions::new(
            ToolKind::ClaudeCode,
            true,
            vec![
                model("default", "Default", &["medium", "high"], "medium"),
                model("opus", "Opus", &["low", "high", "max"], "high"),
            ],
            "default".to_string(),
            Some(false),
        )
    }

    #[test]
    fn an_ordinary_hire_is_unchanged_and_a_fixed_role_refuses_messages() {
        let tool = options();
        let req = HireRequest {
            tool: ToolKind::ClaudeCode,
            title: "fix it".to_string(),
            model: "opus".to_string(),
            effort: "high".to_string(),
            ultracode: true,
            view: crate::model::DeskView::Terminal,
        };
        let mut setup = Launch {
            opts: vec![crate::setup::Opt {
                flag: "--plugin-dir",
                value: "/b/claude-code".to_string(),
            }],
            ..Launch::default()
        };
        let command = launch_command(&req, &tool.models[1], "fix it", "/bin/zsh", &setup);
        assert_eq!(
            command.as_deref(),
            Ok(
                r#"claude --plugin-dir '/b/claude-code' --model 'opus' --effort 'high' --settings '{"ultracode":true}' -- 'fix it'"#
            )
        );
        setup.refuse_peer_messages = true;
        let command = launch_command(&req, &tool.models[1], "fix it", "/bin/zsh", &setup);
        assert_eq!(
            command.as_deref(),
            Ok(
                r#"claude --plugin-dir '/b/claude-code' --model 'opus' --effort 'high' --settings '{"ultracode":true,"crossSessionInbound":"refuse"}' -- 'fix it'"#
            )
        );
    }

    #[test]
    fn with_nothing_chosen_the_default_model_runs_at_its_default_effort() {
        let tool = options();
        let (model, effort) = chosen_model(&tool, None, None).expect("default");
        assert_eq!((model.id.as_str(), effort.as_str()), ("default", "medium"));
        let (model, effort) = chosen_model(&tool, Some("opus"), None).expect("opus");
        assert_eq!((model.id.as_str(), effort.as_str()), ("opus", "high"));
        let (model, effort) = chosen_model(&tool, Some("opus"), Some("max")).expect("max");
        assert_eq!((model.id.as_str(), effort.as_str()), ("opus", "max"));
    }

    #[test]
    fn an_unknown_model_or_effort_is_refused_in_the_hire_dialogs_words() {
        let tool = options();
        assert_eq!(
            chosen_model(&tool, Some("haiku"), None).map(|(m, _)| m.id.clone()),
            Err("haiku is not a model the app can start".to_string())
        );
        assert_eq!(
            chosen_model(&tool, Some("opus"), Some("medium")).map(|(m, _)| m.id.clone()),
            Err("Opus does not support medium effort".to_string())
        );
    }
}
