//! What each harness already has at user level, read from its own files. A
//! list is `None` when the app does not read that tool's files for it yet.
//!
//! Sources, as found on a Mac on 2026-10-08:
//! * Claude Code: `~/.claude/skills/*/SKILL.md`, `~/.claude.json` (`mcpServers`),
//!   `~/.claude/settings.json` (`hooks`, `enabledPlugins`).
//! * Codex: `~/.agents/skills` and `~/.codex/skills`, `~/.codex/config.toml`
//!   (`mcp_servers`, `plugins.<id>.enabled`), `~/.codex/hooks.json` (`hooks`).
//! * Antigravity: `~/.gemini/antigravity-cli/skills`, `~/.gemini/config/mcp_config.json`
//!   (`mcpServers`). Its hooks and plugins are not read yet.
//! * opencode: `~/.config/opencode/skills`, `~/.config/opencode/opencode.json`
//!   or `opencode.jsonc` (`mcp`, `plugin`), per <https://opencode.ai/docs/config/>.
//!   It has no shell hooks (its plugins are code).
//! * Cursor: `~/.cursor/skills` and `~/.agents/skills` (its bundled
//!   `skills-cursor` are left out), `~/.cursor/mcp.json` (`mcpServers`),
//!   `~/.cursor/hooks.json` (`hooks`, Cursor's own event names), local plugins
//!   in `~/.cursor/plugins/local`.

use super::{ItemKind, KINDS, SkillInfo, Support, read_skills, support};
use crate::model::ToolKind;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The harnesses the setup floor lists, in the hire picker's order.
const HARNESSES: [ToolKind; 5] = [
    ToolKind::ClaudeCode,
    ToolKind::Codex,
    ToolKind::Antigravity,
    ToolKind::Opencode,
    ToolKind::Cursor,
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HookInfo {
    /// `<event>:<n>`: the n-th command under that event, counting from 0.
    pub id: String,
    pub event: String,
    pub matcher: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HarnessSetup {
    pub tool: ToolKind,
    pub installed: bool,
    pub skills: Option<Vec<SkillInfo>>,
    pub mcp_servers: Option<Vec<String>>,
    pub hooks: Option<Vec<HookInfo>>,
    pub plugins: Option<Vec<PluginInfo>>,
    /// The tool's user-level instructions file, when it has one (`CLAUDE.md`, `AGENTS.md`).
    pub instructions: Option<Vec<String>>,
    /// What the building can hand this tool, per item kind.
    pub support: BTreeMap<ItemKind, Support>,
}

/// Every file and folder the inventory reads, to watch for changes.
#[must_use]
pub fn sources(home: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = HARNESSES
        .iter()
        .flat_map(|&t| skill_dirs(t, home))
        .collect();
    paths.extend(
        [
            ".claude.json",
            ".claude/settings.json",
            ".codex/config.toml",
            ".codex/hooks.json",
            ".gemini/config/mcp_config.json",
            ".config/opencode",
            ".cursor/mcp.json",
            ".cursor/hooks.json",
            ".cursor/plugins/local",
            ".claude/CLAUDE.md",
            ".codex/AGENTS.md",
            ".config/opencode/AGENTS.md",
            ".gemini/GEMINI.md",
        ]
        .iter()
        .map(|rel| home.join(rel)),
    );
    paths
}

/// One tool's own command hooks, as the inventory lists them.
pub(super) fn hooks_of(tool: ToolKind, home: &Path) -> Vec<HookInfo> {
    let lists = match tool {
        ToolKind::ClaudeCode => claude(home),
        ToolKind::Codex => codex(home),
        ToolKind::Cursor => cursor(home),
        _ => return Vec::new(),
    };
    lists.2.unwrap_or_default()
}

#[must_use]
pub fn harnesses(home: &Path, installed: &[ToolKind]) -> Vec<HarnessSetup> {
    HARNESSES
        .iter()
        .map(|&tool| {
            let (skills, mcp_servers, hooks, plugins) = match tool {
                ToolKind::ClaudeCode => claude(home),
                ToolKind::Codex => codex(home),
                ToolKind::Antigravity => antigravity(home),
                ToolKind::Opencode => opencode(home),
                ToolKind::Cursor => cursor(home),
                ToolKind::Unknown => (None, None, None, None),
            };
            let instructions = instructions_file(tool, home).map(|path| {
                path.is_file()
                    .then(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .flatten()
                    .into_iter()
                    .collect()
            });
            HarnessSetup {
                tool,
                installed: installed.contains(&tool),
                instructions,
                skills,
                mcp_servers,
                hooks,
                plugins,
                support: KINDS.iter().map(|&k| (k, support(tool, k))).collect(),
            }
        })
        .collect()
}

/// Where each tool keeps its user-level skills, in the order it reads them.
pub(super) fn skill_dirs(tool: ToolKind, home: &Path) -> Vec<PathBuf> {
    let rel: &[&str] = match tool {
        ToolKind::ClaudeCode => &[".claude/skills"],
        ToolKind::Codex => &[".agents/skills", ".codex/skills"],
        ToolKind::Antigravity => &[".gemini/antigravity-cli/skills"],
        ToolKind::Opencode => &[".config/opencode/skills"],
        ToolKind::Cursor => &[".cursor/skills", ".agents/skills"],
        ToolKind::Unknown => &[],
    };
    rel.iter().map(|r| home.join(r)).collect()
}

/// A tool's skills across its folders; the same name in two folders is one skill.
fn tool_skills(tool: ToolKind, home: &Path) -> Vec<SkillInfo> {
    let mut skills: Vec<SkillInfo> = Vec::new();
    for dir in skill_dirs(tool, home) {
        for skill in read_skills(&dir) {
            if !skills.iter().any(|s| s.name == skill.name) {
                skills.push(skill);
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

type Lists = (
    Option<Vec<SkillInfo>>,
    Option<Vec<String>>,
    Option<Vec<HookInfo>>,
    Option<Vec<PluginInfo>>,
);

/// `{ "<event>": [{ "matcher": …, "hooks": [{ "type": "command", "command": … }] }] }`,
/// as Claude Code and Codex both write it.
#[derive(Deserialize, Default)]
pub(super) struct HookGroup {
    #[serde(default)]
    matcher: Option<String>,
    #[serde(default)]
    hooks: Vec<HookCommand>,
}

#[derive(Deserialize, Default)]
pub(super) struct HookCommand {
    /// Absent for handler types other than `command` (prompts, agents).
    #[serde(default)]
    command: Option<String>,
}

pub(super) type HookTable = BTreeMap<String, Vec<HookGroup>>;

/// Every command hook in `table`, numbered within its event.
pub(super) fn hook_list(table: HookTable) -> Vec<HookInfo> {
    let mut out = Vec::new();
    for (event, groups) in table {
        let mut n = 0;
        for group in groups {
            for command in group.hooks.into_iter().filter_map(|h| h.command) {
                out.push(HookInfo {
                    id: format!("{event}:{n}"),
                    event: event.clone(),
                    matcher: group.matcher.clone(),
                    command,
                });
                n += 1;
            }
        }
    }
    out
}

/// Where each tool keeps its user-level instructions; `None` when the app does
/// not know (Cursor keeps its user rules in its own settings store).
pub(super) fn instructions_file(tool: ToolKind, home: &Path) -> Option<PathBuf> {
    let rel = match tool {
        ToolKind::ClaudeCode => ".claude/CLAUDE.md",
        ToolKind::Codex => ".codex/AGENTS.md",
        ToolKind::Opencode => ".config/opencode/AGENTS.md",
        ToolKind::Antigravity => ".gemini/GEMINI.md",
        ToolKind::Cursor | ToolKind::Unknown => return None,
    };
    Some(home.join(rel))
}

#[derive(Deserialize, Default)]
struct McpServers {
    #[serde(default, rename = "mcpServers")]
    #[expect(
        clippy::zero_sized_map_values,
        reason = "only the server names are read, their settings are skipped unparsed"
    )]
    mcp_servers: BTreeMap<String, IgnoredAny>,
}

#[derive(Deserialize, Default)]
struct ClaudeSettings {
    #[serde(default)]
    hooks: HookTable,
    #[serde(default, rename = "enabledPlugins")]
    enabled_plugins: BTreeMap<String, bool>,
}

/// A file the tool has not written yet reads as empty, as does one it would not
/// start with either: the inventory only lists, it never blocks.
fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn claude(home: &Path) -> Lists {
    let settings: ClaudeSettings = read_json(&home.join(".claude/settings.json"));
    let user: McpServers = read_json(&home.join(".claude.json"));
    (
        Some(tool_skills(ToolKind::ClaudeCode, home)),
        Some(user.mcp_servers.into_keys().collect()),
        Some(hook_list(settings.hooks)),
        Some(
            settings
                .enabled_plugins
                .into_iter()
                .map(|(name, enabled)| PluginInfo { name, enabled })
                .collect(),
        ),
    )
}

#[derive(Deserialize, Default)]
struct CodexConfig {
    #[serde(default)]
    #[expect(
        clippy::zero_sized_map_values,
        reason = "only the server names are read, their settings are skipped unparsed"
    )]
    mcp_servers: BTreeMap<String, IgnoredAny>,
    #[serde(default)]
    plugins: BTreeMap<String, CodexPlugin>,
}

#[derive(Deserialize)]
struct CodexPlugin {
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Deserialize, Default)]
struct CodexHooks {
    #[serde(default)]
    hooks: HookTable,
}

fn codex(home: &Path) -> Lists {
    let config: CodexConfig = fs::read_to_string(home.join(".codex/config.toml"))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    let hooks: CodexHooks = read_json(&home.join(".codex/hooks.json"));
    (
        Some(tool_skills(ToolKind::Codex, home)),
        Some(config.mcp_servers.into_keys().collect()),
        Some(hook_list(hooks.hooks)),
        Some(
            config
                .plugins
                .into_iter()
                .map(|(name, p)| PluginInfo {
                    name,
                    enabled: p.enabled,
                })
                .collect(),
        ),
    )
}

fn antigravity(home: &Path) -> Lists {
    let mcp: McpServers = read_json(&home.join(".gemini/config/mcp_config.json"));
    (
        Some(tool_skills(ToolKind::Antigravity, home)),
        Some(mcp.mcp_servers.into_keys().collect()),
        None,
        None,
    )
}

#[derive(Deserialize, Default)]
struct OpencodeConfig {
    #[expect(
        clippy::zero_sized_map_values,
        reason = "only the server names are read, their settings are skipped unparsed"
    )]
    #[serde(default)]
    mcp: BTreeMap<String, IgnoredAny>,
    #[serde(default)]
    plugin: Vec<String>,
}

fn opencode(home: &Path) -> Lists {
    let dir = home.join(".config/opencode");
    let config: OpencodeConfig = ["opencode.json", "opencode.jsonc"]
        .iter()
        .find_map(|name| fs::read_to_string(dir.join(name)).ok())
        .and_then(|text| serde_json::from_str(&strip_json_comments(&text)).ok())
        .unwrap_or_default();
    (
        Some(tool_skills(ToolKind::Opencode, home)),
        Some(config.mcp.into_keys().collect()),
        Some(Vec::new()),
        Some(
            config
                .plugin
                .into_iter()
                .map(|name| PluginInfo {
                    name,
                    enabled: true,
                })
                .collect(),
        ),
    )
}

/// JSON with `//` and `/* */` comments (JSONC) as plain JSON. Text inside
/// strings is kept as it is.
pub(super) fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for skipped in chars.by_ref() {
                    if last == '*' && skipped == '/' {
                        break;
                    }
                    last = skipped;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Cursor's `hooks.json`: `{ "version": 1, "hooks": { "<event>": [{ "command": … }] } }`.
#[derive(Deserialize, Default)]
struct CursorHooks {
    #[serde(default)]
    hooks: BTreeMap<String, Vec<HookCommand>>,
}

fn cursor(home: &Path) -> Lists {
    let mcp: McpServers = read_json(&home.join(".cursor/mcp.json"));
    let hooks: CursorHooks = read_json(&home.join(".cursor/hooks.json"));
    let mut plugins: Vec<PluginInfo> = fs::read_dir(home.join(".cursor/plugins/local"))
        .map(|it| {
            it.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| PluginInfo {
                    name: e.file_name().to_string_lossy().into_owned(),
                    enabled: true,
                })
                .collect()
        })
        .unwrap_or_default();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    (
        Some(tool_skills(ToolKind::Cursor, home)),
        Some(mcp.mcp_servers.into_keys().collect()),
        // Cursor lists commands straight under each event, with no matcher groups.
        Some(hook_list(
            hooks
                .hooks
                .into_iter()
                .map(|(event, hooks)| {
                    let group = HookGroup {
                        matcher: None,
                        hooks,
                    };
                    (event, vec![group])
                })
                .collect(),
        )),
        Some(plugins),
    )
}
