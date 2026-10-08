//! Which agent tools are installed: a tool is installed when its CLI is on the
//! login shell's PATH (the same PATH a hired agent's terminal gets).

use crate::model::ToolKind;
use crate::pty;
use std::process::Command;

/// Every tool the hire picker lists, in its order. `Unknown` is never listed.
pub const LISTED: &[ToolKind] = &[
    ToolKind::ClaudeCode,
    ToolKind::Codex,
    ToolKind::Antigravity,
    ToolKind::Opencode,
    ToolKind::Cursor,
];

/// The command a tool is started with.
#[must_use]
pub fn binary(tool: ToolKind) -> Option<&'static str> {
    match tool {
        ToolKind::ClaudeCode => Some("claude"),
        ToolKind::Codex => Some("codex"),
        ToolKind::Antigravity => Some("agy"),
        ToolKind::Opencode => Some("opencode"),
        ToolKind::Cursor => Some("cursor-agent"),
        ToolKind::Unknown => None,
    }
}

/// The listed tools whose CLI the login shell finds, in one shell run.
#[must_use]
pub fn installed() -> Vec<ToolKind> {
    let names: Vec<&str> = LISTED.iter().filter_map(|t| binary(*t)).collect();
    let script = names
        .iter()
        .map(|n| format!("command -v {n} >/dev/null 2>&1 && echo {n}"))
        .collect::<Vec<_>>()
        .join("; ");
    let found = Command::new(pty::login_shell())
        .args(["-l", "-c", &script])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let found: Vec<&str> = found.lines().map(str::trim).collect();
    LISTED
        .iter()
        .copied()
        .filter(|t| binary(*t).is_some_and(|b| found.contains(&b)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_tool_has_a_binary() {
        assert!(LISTED.iter().all(|t| binary(*t).is_some()));
        assert_eq!(binary(ToolKind::Unknown), None);
    }
}
