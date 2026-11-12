//! Agents the building starts for a fixed job, with a role's own instructions, permissions,
//! folder and report file. The QA reviewer is the first. Issue #4's manager is another.

use super::Office;
use super::launch::{chosen_model, hireable, launch_command};
use crate::model::{HireRequest, ModelOption, ToolKind};
use crate::pty;
use crate::setup::{Hook, HookEvent, Launch, Opt, RoleSetup, Targets};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// What a fixed-role agent may do without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// Reads and searches files in its folder. No shell, no edits, no web, no MCP tools,
    /// and anything that would ask is refused.
    ReadOnly,
}

/// An agent the building starts for a fixed job. Issue #4's manager is another one.
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
}

/// The options that put `tool` in `mode`: Claude Code's `dontAsk` mode with only the file
/// reading and search tools, no MCP servers and no project settings (`claude --help`,
/// <https://code.claude.com/docs/en/permission-modes>).
pub(super) fn permission_options(
    tool: ToolKind,
    mode: PermissionMode,
    launch: &mut Launch,
) -> Result<(), String> {
    if tool != ToolKind::ClaudeCode {
        return Err("only Claude Code can take a fixed job so far".to_string());
    }
    match mode {
        PermissionMode::ReadOnly => {
            for (flag, value) in [
                ("--permission-mode", "dontAsk"),
                ("--tools", "Read,Grep,Glob"),
                ("--setting-sources", "user"),
            ] {
                launch.opts.push(Opt {
                    flag,
                    value: value.to_string(),
                });
            }
            launch.flags.push("--strict-mcp-config");
        }
    }
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
        let setup = self.setup.role_launch(
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
            },
            Some(&self.lock().feed),
        )?;
        staff_command(role, tool, model, &effort, task, &pty::login_shell(), setup)
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
        assert_eq!(
            permission_options(ToolKind::Codex, PermissionMode::ReadOnly, &mut launch),
            Err("only Claude Code can take a fixed job so far".to_string())
        );
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
