//! Agents the building starts for a fixed job, with a role's own instructions, permissions,
//! folder and report file: the QA reviewer and the floor's manager.

use super::Office;
use super::launch::{chosen_model, hireable, launch_command};
use crate::manager::helper;
use crate::model::{HireRequest, ModelOption, ToolKind};
use crate::pty;
use crate::setup::{Hook, HookEvent, Launch, Opt, RoleServer, RoleSetup, Targets};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The name the building's tools are served under: the allow rule and the server config share it.
const SERVER: &str = "office";

/// What a fixed-role agent may do without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// Reads and searches files in its folder. No shell, no edits, no web, no MCP tools,
    /// and anything that would ask is refused.
    ReadOnly,
    /// Reads and searches files in its folder and uses the building's tools. No shell, no
    /// edits, no web, no other MCP server.
    Manager,
}

/// The helper a role's agent reaches the building through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpHelper {
    pub exe: PathBuf,
    pub socket: PathBuf,
}

/// An agent the building starts for a fixed job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    /// Names its generated launch folder: see `RoleSetup::name`.
    pub name: String,
    /// Added to the tool's system prompt.
    pub instructions: String,
    pub permissions: PermissionMode,
    /// The folder it starts in.
    pub folder: PathBuf,
    /// Where the last reply of each turn is written (the Stop hook's input), if anywhere.
    pub report: Option<PathBuf>,
    /// The building's tools, through this helper, if it has them.
    pub helper: Option<McpHelper>,
}

/// The options that put `tool` in `mode`: Claude Code's `dontAsk` mode with only the file
/// reading and search tools, no project settings, no MCP server but the building's for a
/// manager, and no messages from other sessions (`claude --help`,
/// <https://code.claude.com/docs/en/permission-modes>, `permissions`, `mcp`).
pub(super) fn permission_options(
    tool: ToolKind,
    mode: PermissionMode,
    launch: &mut Launch,
) -> Result<(), String> {
    if tool != ToolKind::ClaudeCode {
        return Err("only Claude Code can take a fixed job so far".to_string());
    }
    let mut opts = vec![
        ("--permission-mode", "dontAsk".to_string()),
        ("--tools", "Read,Grep,Glob".to_string()),
        ("--setting-sources", "user".to_string()),
    ];
    match mode {
        PermissionMode::ReadOnly => {}
        PermissionMode::Manager => opts.push(("--allowedTools", format!("mcp__{SERVER}__*"))),
    }
    for (flag, value) in opts {
        launch.opts.push(Opt { flag, value });
    }
    launch.flags.push("--strict-mcp-config");
    launch.refuse_peer_messages = true;
    Ok(())
}

/// The Stop hook that writes its input to `report`, whole or not at all.
pub(super) fn report_hook(report: &Path) -> Hook {
    let q = |p: &str| pty::quote("/bin/sh", p);
    let path = report.to_string_lossy();
    let part = q(&format!("{path}.part"));
    Hook {
        event: HookEvent::Stop,
        matcher: None,
        command: format!("cat > {part} && mv {part} {}", q(&path)),
        targets: Targets::All,
    }
}

/// The shell command that starts `role` on `task`: the ordinary hire command with the
/// role's options in front of `--model`.
pub(super) fn staff_command(
    role: &Role,
    tool: ToolKind,
    model: &ModelOption,
    effort: &str,
    task: &str,
    shell: &str,
    mut setup: Launch,
) -> Result<String, String> {
    permission_options(tool, role.permissions, &mut setup)?;
    let req = HireRequest {
        tool,
        title: task.to_string(),
        model: model.id.clone(),
        effort: effort.to_string(),
        ultracode: false,
        structured: false,
    };
    launch_command(&req, model, task, shell, &setup)
}

impl Office {
    /// The command that starts `role` on `task`, with its launch folder written and a stale
    /// report deleted: everything a hire does before the terminal opens.
    pub(super) fn staff_launch(
        &self,
        role: &Role,
        tool: ToolKind,
        model: Option<&str>,
        effort: Option<&str>,
        task: &str,
    ) -> Result<String, String> {
        let options = hireable(self.hire_options(), tool)?;
        let (model, effort) = chosen_model(&options, model, effort)?;
        let setup = self.role_setup(role, tool)?;
        staff_command(role, tool, model, &effort, task, &pty::login_shell(), setup)
    }

    /// The role's launch folder written and a stale report deleted.
    pub(super) fn role_setup(&self, role: &Role, tool: ToolKind) -> Result<Launch, String> {
        if let Some(report) = &role.report {
            if let Some(parent) = report.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
            }
            match fs::remove_file(report) {
                Err(e) if e.kind() != ErrorKind::NotFound => {
                    return Err(format!("could not clear {}: {e}", report.display()));
                }
                Ok(()) | Err(_) => {}
            }
        }
        let args = |h: &McpHelper| {
            vec![
                helper::MODE.to_string(),
                h.socket.to_string_lossy().into_owned(),
            ]
        };
        self.setup.role_launch(
            tool,
            &RoleSetup {
                name: &role.name,
                instructions: &role.instructions,
                hooks: role
                    .report
                    .as_deref()
                    .map(report_hook)
                    .into_iter()
                    .collect(),
                server: role.helper.as_ref().map(|h| RoleServer {
                    name: SERVER,
                    command: &h.exe,
                    args: args(h),
                }),
            },
            Some(&self.lock().feed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EffortOption;
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn role() -> Role {
        Role {
            name: "qa-p-1".to_string(),
            instructions: "Review.".to_string(),
            permissions: PermissionMode::ReadOnly,
            folder: PathBuf::from("/tmp/wt"),
            report: None,
            helper: None,
        }
    }

    fn model() -> ModelOption {
        ModelOption {
            id: "opus".to_string(),
            label: "Opus".to_string(),
            efforts: vec![EffortOption {
                id: "high".to_string(),
                label: "High".to_string(),
                description: String::new(),
            }],
            default_effort: "high".to_string(),
        }
    }

    #[test]
    fn read_only_adds_exactly_three_options_and_one_flag() {
        let mut launch = Launch::default();
        permission_options(ToolKind::ClaudeCode, PermissionMode::ReadOnly, &mut launch)
            .expect("options");
        let opts: Vec<(&str, &str)> = launch
            .opts
            .iter()
            .map(|o| (o.flag, o.value.as_str()))
            .collect();
        assert_eq!(
            opts,
            vec![
                ("--permission-mode", "dontAsk"),
                ("--tools", "Read,Grep,Glob"),
                ("--setting-sources", "user"),
            ]
        );
        assert_eq!(launch.flags, vec!["--strict-mcp-config"]);
        assert!(launch.env.is_empty());
        assert!(launch.refuse_peer_messages);
        assert_eq!(
            permission_options(ToolKind::Codex, PermissionMode::ReadOnly, &mut launch),
            Err("only Claude Code can take a fixed job so far".to_string())
        );
    }

    #[test]
    fn a_manager_adds_the_buildings_tools_and_nothing_else() {
        let mut launch = Launch::default();
        permission_options(ToolKind::ClaudeCode, PermissionMode::Manager, &mut launch)
            .expect("options");
        let opts: Vec<(&str, &str)> = launch
            .opts
            .iter()
            .map(|o| (o.flag, o.value.as_str()))
            .collect();
        assert_eq!(
            opts,
            vec![
                ("--permission-mode", "dontAsk"),
                ("--tools", "Read,Grep,Glob"),
                ("--setting-sources", "user"),
                ("--allowedTools", "mcp__office__*"),
            ]
        );
        assert_eq!(launch.flags, vec!["--strict-mcp-config"]);
        assert!(launch.env.is_empty());
        assert!(launch.refuse_peer_messages);
    }

    #[test]
    fn the_managers_command_reads_files_and_calls_the_building_only() {
        let setup = Launch {
            opts: vec![Opt {
                flag: "--mcp-config",
                value: "/b/roles/manager-p-1/mcp.json".to_string(),
            }],
            ..Launch::default()
        };
        let manager = Role {
            name: "manager-p-1".to_string(),
            permissions: PermissionMode::Manager,
            ..role()
        };
        let command = staff_command(
            &manager,
            ToolKind::ClaudeCode,
            &model(),
            "high",
            "Call list_desks.",
            "/bin/zsh",
            setup,
        )
        .expect("command");
        let model_at = command.find(" --model ").expect("model");
        for option in [
            "--mcp-config '/b/roles/manager-p-1/mcp.json'",
            "--permission-mode 'dontAsk'",
            "--tools 'Read,Grep,Glob'",
            "--setting-sources 'user'",
            "--allowedTools 'mcp__office__*'",
            "--strict-mcp-config",
        ] {
            let at = command.find(option).expect(option);
            assert!(at < model_at, "{option} comes before --model");
        }
        assert_eq!(command.matches("--strict-mcp-config").count(), 1);
        assert!(
            command.contains(r#"--settings '{"ultracode":false,"crossSessionInbound":"refuse"}'"#)
        );
        for banned in [
            "Bash",
            "Edit",
            "Write",
            "SendMessage",
            "bypassPermissions",
            "--dangerously-skip-permissions",
        ] {
            assert!(!command.contains(banned), "{banned} in {command}");
        }
    }

    #[test]
    fn the_staff_command_is_an_interactive_read_only_hire() {
        let setup = Launch {
            opts: vec![Opt {
                flag: "--plugin-dir",
                value: "/b/roles/qa-p-1/claude-code".to_string(),
            }],
            ..Launch::default()
        };
        let command = staff_command(
            &role(),
            ToolKind::ClaudeCode,
            &model(),
            "high",
            "review it's diff",
            "/bin/zsh",
            setup,
        )
        .expect("command");
        let model_at = command.find(" --model ").expect("model");
        for option in [
            "--plugin-dir",
            "--permission-mode 'dontAsk'",
            "--tools 'Read,Grep,Glob'",
            "--setting-sources 'user'",
            "--strict-mcp-config",
        ] {
            let at = command.find(option).expect(option);
            assert!(at < model_at, "{option} comes before --model");
        }
        assert!(command.ends_with(r" -- 'review it'\''s diff'"));
        for banned in [
            "-p ",
            "--print",
            "bypassPermissions",
            "--dangerously-skip-permissions",
            "acceptEdits",
        ] {
            assert!(!command.contains(banned), "{banned} in {command}");
        }
        assert_eq!(command.matches("--permission-mode").count(), 1);
        assert!(
            command.contains(r#"--settings '{"ultracode":false,"crossSessionInbound":"refuse"}'"#)
        );
        assert!(!command.contains("--allowedTools"));
    }

    #[test]
    fn the_report_hook_writes_its_input_whole_through_a_part_file() {
        let dir = std::env::temp_dir().join(format!("ob-staff-{}", std::process::id()));
        let report = dir.join("it's a run").join("report.json");
        std::fs::create_dir_all(report.parent().expect("parent")).expect("mkdir");
        let hook = report_hook(&report);
        assert_eq!(hook.event, HookEvent::Stop);
        assert_eq!(hook.matcher, None);
        assert!(
            hook.command
                .contains(r"it'\''s a run/report.json.part' && mv '")
        );
        let input = r#"{"last_assistant_message":"done"}"#;
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(&hook.command)
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .spawn()
            .expect("sh");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write");
        assert!(child.wait().expect("wait").success());
        assert_eq!(std::fs::read_to_string(&report).expect("report"), input);
        assert!(!report.with_extension("json.part").exists());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
