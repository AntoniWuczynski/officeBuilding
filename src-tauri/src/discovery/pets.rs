//! What an agent left running beside its desk, from one `ps` listing: a shell
//! (a background command, or a long one still going), a monitor (Claude Code's
//! `Monitor` tool), a browser it drives, or any other program it started.
//!
//! Only an agent's own children count, each with whatever it started in turn.
//! Left out: children started together with the agent (its MCP servers and
//! other start-up helpers, seen on this Mac 2026-10-09 within 15 s of the
//! agent), anything naming MCP, Claude Code's `caffeinate`, Codex's
//! `codex-code-mode-host`, and commands younger than a few seconds, so a quick
//! tool call never flashes a pet.

use crate::model::{Pet, PetKind};
use std::process::Command;
use std::time::Duration;

/// A child this young may still be a quick tool call.
const SETTLE: Duration = Duration::from_secs(5);
/// A child started this soon after its agent came up with it.
const STARTUP: Duration = Duration::from_secs(20);
/// Longest label shown under a pet.
const LABEL_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRow {
    pub pid: i32,
    pub parent: i32,
    pub age: Duration,
    pub command: String,
}

/// A `Monitor` call from a Claude Code transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorCall {
    pub command: String,
    pub description: String,
}

/// Every process with its parent, age and full command line. Never panics: an
/// unavailable `ps` just yields none.
#[must_use]
pub fn read_table() -> Vec<ProcessRow> {
    Command::new("ps")
        .args(["-axww", "-o", "pid=,ppid=,etime=,command="])
        .output()
        .map(|out| parse_table(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

fn parse_table(text: &str) -> Vec<ProcessRow> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse().ok()?;
            let parent = parts.next()?.parse().ok()?;
            let age = parse_etime(parts.next()?)?;
            let command = parts.collect::<Vec<_>>().join(" ");
            Some(ProcessRow {
                pid,
                parent,
                age,
                command,
            })
        })
        .collect()
}

/// `ps`'s elapsed time: `[[dd-]hh:]mm:ss`.
fn parse_etime(text: &str) -> Option<Duration> {
    let (days, clock) = match text.split_once('-') {
        Some((d, rest)) => (d.parse::<u64>().ok()?, rest),
        None => (0, text),
    };
    let mut secs = 0u64;
    for part in clock.split(':') {
        secs = secs * 60 + part.parse::<u64>().ok()?;
    }
    Some(Duration::from_secs(days * 86_400 + secs))
}

/// How Claude Code's shell wrapper spells a command: inside `eval '…'`, each
/// quote closed, escaped and reopened, and `ps` shows a newline as `\012`.
fn as_evaluated(command: &str) -> String {
    format!(
        "eval '{}'",
        command.replace('\'', r#"'"'"'"#).replace('\n', r"\012")
    )
}

/// The command a Claude Code shell wrapper evaluates, unescaped, when `line` is one.
fn evaluated(line: &str) -> Option<String> {
    let start = line.find("eval '")? + "eval '".len();
    let end = line.rfind("' < /dev/null").or_else(|| line.rfind('\''))?;
    (end > start).then(|| {
        line[start..end]
            .replace(r#"'"'"'"#, "'")
            .replace(r"\012", "\n")
    })
}

fn short(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= LABEL_CHARS {
        flat
    } else {
        let cut: String = flat.chars().take(LABEL_CHARS - 1).collect();
        format!("{cut}…")
    }
}

fn program(command: &str) -> &str {
    command
        .split_whitespace()
        .next()
        .and_then(|first| first.rsplit('/').next())
        .unwrap_or_default()
}

fn is_infrastructure(command: &str) -> bool {
    command.to_ascii_lowercase().contains("mcp")
        || matches!(program(command), "caffeinate" | "codex-code-mode-host")
}

fn classify(row: &ProcessRow, monitors: &[MonitorCall]) -> Pet {
    let id = format!("pet-{}", row.pid);
    if matches!(
        program(&row.command),
        "zsh" | "bash" | "sh" | "dash" | "fish"
    ) {
        if let Some(m) = monitors
            .iter()
            .find(|m| row.command.contains(&as_evaluated(&m.command)))
        {
            return Pet {
                id,
                kind: PetKind::Monitor,
                label: short(&m.description),
            };
        }
        let label = evaluated(&row.command).unwrap_or_else(|| row.command.clone());
        return Pet {
            id,
            kind: PetKind::Shell,
            label: short(&label),
        };
    }
    let browser = ["node_repl", "Chrome", "Chromium", "firefox", "WebKit"]
        .iter()
        .any(|name| row.command.contains(name));
    Pet {
        id,
        kind: if browser {
            PetKind::Browser
        } else {
            PetKind::Process
        },
        label: short(program(&row.command)),
    }
}

/// The pets of the agent running as `agent`, oldest first.
#[must_use]
pub fn pets_of(agent: i32, table: &[ProcessRow], monitors: &[MonitorCall]) -> Vec<Pet> {
    let Some(me) = table.iter().find(|r| r.pid == agent) else {
        return Vec::new();
    };
    let mut kids: Vec<&ProcessRow> = table
        .iter()
        .filter(|r| r.parent == agent)
        .filter(|r| r.age >= SETTLE)
        .filter(|r| me.age.saturating_sub(r.age) >= STARTUP)
        .filter(|r| !is_infrastructure(&r.command))
        .collect();
    kids.sort_by_key(|r| std::cmp::Reverse(r.age));
    kids.into_iter().map(|r| classify(r, monitors)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shapes seen on this Mac, 2026-10-09 (paths shortened).
    const PS: &str = "\
  100     1 02:00:00 claude
  101   100 01:59:58 npm exec @playwright/mcp@latest
  102   100 01:59:58 /agent/.venv/bin/python3 /agent/graphify serve
  103   100    02:05 caffeinate -i -t 300
  104   100    10:00 /bin/zsh -c source /h/.claude/shell-snapshots/s.sh 2>/dev/null || true && eval 'pnpm dev --port 5183' < /dev/null && pwd -P >| /tmp/cwd
  105   100    09:00 /bin/zsh -c source /h/.claude/shell-snapshots/s.sh 2>/dev/null || true && eval 'tail -f \"$L\" | grep -E '\"'\"'^error'\"'\"'' < /dev/null && pwd -P >| /tmp/cwd
  106   100    00:02 /bin/zsh -c source /h/.claude/shell-snapshots/s.sh && eval 'ls' < /dev/null
  200     1 1-00:49:42 codex -m gpt
  201   200 1-00:49:23 /h/.codex/packages/bin/codex-code-mode-host
  202   200    14:14:45 /Applications/ChatGPT.app/Contents/Resources/cua_node/bin/node_repl
  203   200    14:14:45 npm exec @playwright/mcp@latest
  204   202    14:14:40 /Applications/ChatGPT.app/codex sandbox -- node kernel.js
  205   200    03:00 /Users/me/bin/devserver --watch
";

    fn table() -> Vec<ProcessRow> {
        parse_table(PS)
    }

    #[test]
    fn elapsed_times_parse_with_days_and_hours() {
        assert_eq!(parse_etime("02:05"), Some(Duration::from_secs(125)));
        assert_eq!(parse_etime("01:00:00"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_etime("1-00:00:01"), Some(Duration::from_secs(86_401)));
        assert_eq!(parse_etime("x"), None);
    }

    #[test]
    fn a_claude_session_keeps_its_shells_and_monitors_but_not_its_start_up_helpers() {
        let monitors = [MonitorCall {
            command: "tail -f \"$L\" | grep -E '^error'".to_string(),
            description: "deploy errors".to_string(),
        }];
        let pets = pets_of(100, &table(), &monitors);
        assert_eq!(
            pets,
            vec![
                Pet {
                    id: "pet-104".to_string(),
                    kind: PetKind::Shell,
                    label: "pnpm dev --port 5183".to_string()
                },
                Pet {
                    id: "pet-105".to_string(),
                    kind: PetKind::Monitor,
                    label: "deploy errors".to_string()
                },
            ]
        );
    }

    #[test]
    fn a_codex_session_gets_a_browser_pet_and_a_pet_for_any_other_program() {
        let kinds: Vec<(String, PetKind)> = pets_of(200, &table(), &[])
            .into_iter()
            .map(|p| (p.label, p.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("node_repl".to_string(), PetKind::Browser),
                ("devserver".to_string(), PetKind::Process),
            ]
        );
    }

    #[test]
    fn an_agent_no_longer_running_has_no_pets() {
        assert!(pets_of(999, &table(), &[]).is_empty());
    }

    #[test]
    fn a_long_command_is_cut_short() {
        let label = short(&"x".repeat(100));
        assert_eq!(label.chars().count(), LABEL_CHARS);
        assert!(label.ends_with('…'));
    }
}
