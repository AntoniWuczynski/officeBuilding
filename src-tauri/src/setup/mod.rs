//! The setup floor: the building's own skills, instructions, MCP servers and
//! hooks, each aimed at chosen harnesses, plus a read-only inventory of what
//! each harness already has. Nothing here writes outside the app's data
//! directory: the building's items reach a tool only through what it is
//! launched with (`inject`).
//!
//! Layout of the library folder (the setup floor's own folder):
//! * `skills/<name>/SKILL.md`: one folder per skill, in the Agent Skills format.
//! * `instructions/<name>.md`: standing instructions, added to each tool's own.
//! * `setup.json`: the MCP servers (stdio), the hooks, and every item's targets.

mod files;
mod import;
mod inject;
pub use inject::{Launch, Opt};
mod inventory;
#[cfg(test)]
mod tests;

use crate::model::ToolKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Written once, so agents hired on the setup floor know what the folder is.
const AGENTS_GUIDE: &str = "\
# Office Building setup

This folder is Office Building's setup floor. What is here reaches the agents the
building launches, and only those: the building passes it to each tool at launch
and never writes into a tool's own config folders.

- `skills/<name>/SKILL.md`: one folder per skill, in the Agent Skills format
  (front matter with `name` and `description`, then the instructions).
- `instructions/<name>.md`: standing instructions, added to each tool's own
  (CLAUDE.md, AGENTS.md and so on).
- `setup.json`: MCP servers, hooks, and which tools each item applies to.

```json
{
  \"skills\": { \"<name>\": { \"targets\": { \"kind\": \"all\" } } },
  \"instructions\": { \"<name>\": { \"targets\": { \"kind\": \"all\" } } },
  \"mcpServers\": {
    \"<name>\": {
      \"command\": \"npx\",
      \"args\": [\"-y\", \"some-server\"],
      \"env\": {},
      \"targets\": { \"kind\": \"only\", \"tools\": [\"claude-code\", \"codex\"] }
    },
    \"<another-name>\": {
      \"url\": \"https://example.com/mcp\",
      \"headers\": { \"Authorization\": \"Bearer …\" },
      \"targets\": { \"kind\": \"all\" }
    }
  },
  \"hooks\": {
    \"<name>\": {
      \"event\": \"PostToolUse\",
      \"matcher\": \"Edit|Write\",
      \"command\": \"./format.sh\",
      \"targets\": { \"kind\": \"all\" }
    }
  }
}
```

A skill or instructions file with no entry applies to every tool. MCP server
names use only letters, digits, `-` and `_`. Hook events: `PreToolUse`,
`PostToolUse`, `UserPromptSubmit`, `Stop`, `SessionStart`, and `Notification`
(Claude Code only); a hook gets the event as JSON on standard input, as Claude
Code and Codex hooks do. Tools:
`claude-code`, `codex`, `antigravity`, `opencode`, `cursor`.
";

/// Which harnesses an item applies to.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Targets {
    #[default]
    All,
    Only {
        tools: Vec<ToolKind>,
    },
}

impl Targets {
    fn includes(&self, tool: ToolKind) -> bool {
        match self {
            Self::All => true,
            Self::Only { tools } => tools.contains(&tool),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    Skill,
    Instructions,
    McpServer,
    Hook,
}

const KINDS: [ItemKind; 4] = [
    ItemKind::Skill,
    ItemKind::Instructions,
    ItemKind::McpServer,
    ItemKind::Hook,
];

/// Whether a harness can take an item kind from the building at launch.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Support {
    /// `note` says what the human still has to do, if anything.
    Supported {
        note: Option<String>,
    },
    Unsupported {
        reason: String,
    },
}

impl Support {
    fn is_supported(&self) -> bool {
        matches!(self, Self::Supported { .. })
    }
}

/// Checked by hand on 2026-10-08 (docs/harness-hub-plan.md).
#[must_use]
pub fn support(tool: ToolKind, kind: ItemKind) -> Support {
    let yes = Support::Supported { note: None };
    let no = |reason: &str| Support::Unsupported {
        reason: reason.to_string(),
    };
    match (tool, kind) {
        (ToolKind::Codex, ItemKind::Hook) => Support::Supported {
            note: Some("Codex skips a new hook until you trust it once with /hooks".to_string()),
        },
        (ToolKind::ClaudeCode | ToolKind::Codex, _)
        | (ToolKind::Opencode, ItemKind::Skill | ItemKind::Instructions | ItemKind::McpServer)
        | (ToolKind::Antigravity, ItemKind::Skill | ItemKind::Instructions)
        | (ToolKind::Cursor, ItemKind::Skill) => yes,
        (ToolKind::Antigravity, ItemKind::McpServer) => {
            no("Antigravity has no launch option for MCP servers")
        }
        (ToolKind::Antigravity, ItemKind::Hook) => no("Antigravity has no launch option for hooks"),
        (ToolKind::Opencode, ItemKind::Hook) => {
            no("opencode's hooks are code plugins, not commands")
        }
        (ToolKind::Cursor, _) => {
            no("Cursor's --plugin-dir carries skills only (checked 2026-10-08)")
        }
        (ToolKind::Unknown, _) => no("Unknown tool"),
    }
}

/// Hook events by the names Claude Code and Codex use. `Notification` is
/// Claude Code's alone (Codex has no such hook).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    UserPromptSubmit,
    Stop,
    SessionStart,
    Notification,
}

impl HookEvent {
    const ALL: [Self; 6] = [
        Self::PreToolUse,
        Self::PostToolUse,
        Self::UserPromptSubmit,
        Self::Stop,
        Self::SessionStart,
        Self::Notification,
    ];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::Stop => "Stop",
            Self::SessionStart => "SessionStart",
            Self::Notification => "Notification",
        }
    }

    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.name() == name)
    }

    /// Whether `tool` has this hook event.
    fn exists_in(self, tool: ToolKind) -> bool {
        !(self == Self::Notification && tool == ToolKind::Codex)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub event: HookEvent,
    /// Tool names it applies to (a pattern, as in Claude Code's hooks); every tool when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub command: String,
    #[serde(default)]
    pub targets: Targets,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    skills: BTreeMap<String, Aim>,
    #[serde(default)]
    instructions: BTreeMap<String, Aim>,
    #[serde(default)]
    mcp_servers: BTreeMap<String, McpServer>,
    #[serde(default)]
    hooks: BTreeMap<String, Hook>,
}

/// The targets of an item whose content lives in its own file (a skill, instructions).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Aim {
    #[serde(default)]
    targets: Targets,
}

/// An MCP server the building hands out: one it starts with a command, or one
/// reached over streamable HTTP, the remote transport Claude Code, Codex and
/// opencode all take.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum McpServer {
    Command(CommandServer),
    Http(HttpServer),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommandServer {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub targets: Targets,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpServer {
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub targets: Targets,
}

impl McpServer {
    #[must_use]
    pub fn targets(&self) -> &Targets {
        match self {
            Self::Command(s) => &s.targets,
            Self::Http(s) => &s.targets,
        }
    }

    fn targets_mut(&mut self) -> &mut Targets {
        match self {
            Self::Command(s) => &mut s.targets,
            Self::Http(s) => &mut s.targets,
        }
    }

    /// Its command, or its address.
    fn description(&self) -> &str {
        match self {
            Self::Command(s) => &s.command,
            Self::Http(s) => &s.url,
        }
    }
}

/// A skill folder: its folder name and the description from its front matter.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

/// One of the building's items, as the Setup panel lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub kind: ItemKind,
    pub name: String,
    /// A skill's description, the first line of instructions, a server's command, a hook's event and command.
    pub description: String,
    pub targets: Targets,
}

/// Everything the Setup panel shows.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SetupView {
    pub items: Vec<LibraryItem>,
    pub harnesses: Vec<inventory::HarnessSetup>,
    /// What in the library could not be used, in words.
    pub problems: Vec<String>,
}

/// The library and the folder the per-tool launch folders are generated in.
pub struct Library {
    dir: PathBuf,
    build: PathBuf,
}

/// What the library holds once read: problems are reported, not fatal.
struct Loaded {
    skills: Vec<SkillInfo>,
    /// Instruction files by name, with their text.
    instructions: Vec<(String, String)>,
    manifest: Manifest,
    problems: Vec<String>,
}

impl Loaded {
    fn aim<'a>(aims: &'a BTreeMap<String, Aim>, name: &str) -> &'a Targets {
        static ALL: Targets = Targets::All;
        aims.get(name).map_or(&ALL, |a| &a.targets)
    }

    /// The names of the skills aimed at `tool`.
    fn skills_for(&self, tool: ToolKind) -> Vec<&str> {
        self.skills
            .iter()
            .filter(|s| Self::aim(&self.manifest.skills, &s.name).includes(tool))
            .map(|s| s.name.as_str())
            .collect()
    }

    /// The instruction files aimed at `tool`, as (name, text).
    fn instructions_for(&self, tool: ToolKind) -> Vec<(&str, &str)> {
        self.instructions
            .iter()
            .filter(|(name, _)| Self::aim(&self.manifest.instructions, name).includes(tool))
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect()
    }
}

impl Library {
    #[must_use]
    pub fn new(dir: PathBuf, build: PathBuf) -> Self {
        Self { dir, build }
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Create the folder on first run, with an empty `setup.json` and a guide.
    pub fn ensure(&self) -> Result<(), String> {
        for sub in ["skills", "instructions"] {
            let dir = self.dir.join(sub);
            fs::create_dir_all(&dir)
                .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let manifest = self.manifest_path();
        if !manifest.exists() {
            write_manifest(&manifest, &Manifest::default())?;
        }
        for (name, text) in [("AGENTS.md", AGENTS_GUIDE), ("CLAUDE.md", "@AGENTS.md\n")] {
            let path = self.dir.join(name);
            if !path.exists() {
                fs::write(&path, text)
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
            }
        }
        Ok(())
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir.join("setup.json")
    }

    fn read_manifest(&self) -> Result<Manifest, String> {
        let path = self.manifest_path();
        match fs::read_to_string(&path) {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|e| format!("setup.json is not valid: {e}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
            Err(e) => Err(format!("could not read {}: {e}", path.display())),
        }
    }

    fn load(&self) -> Loaded {
        let skills = read_skills(&self.dir.join("skills"));
        let instructions = read_instructions(&self.dir.join("instructions"));
        let (mut manifest, mut problems) = match self.read_manifest() {
            Ok(m) => (m, Vec::new()),
            Err(e) => (Manifest::default(), vec![e]),
        };
        manifest.mcp_servers.retain(|name, _| {
            let ok = is_bare_key(name);
            if !ok {
                problems.push(format!(
                    "MCP server “{name}” is skipped: use only letters, digits, - and _ in its name"
                ));
            }
            ok
        });
        Loaded {
            skills,
            instructions,
            manifest,
            problems,
        }
    }

    /// What to watch so the Setup panel follows changes: the library and
    /// every file the inventory reads.
    #[must_use]
    pub fn watched(&self, home: &Path) -> Vec<PathBuf> {
        let mut paths = inventory::sources(home);
        paths.push(self.dir.clone());
        paths
    }

    #[must_use]
    pub fn view(&self, home: &Path, installed: &[ToolKind]) -> SetupView {
        let loaded = self.load();
        let item = |kind, name: &str, description: String, targets: &Targets| LibraryItem {
            kind,
            name: name.to_string(),
            description,
            targets: targets.clone(),
        };
        let m = &loaded.manifest;
        let mut items: Vec<LibraryItem> = loaded
            .skills
            .iter()
            .map(|s| {
                let targets = Loaded::aim(&m.skills, &s.name);
                item(ItemKind::Skill, &s.name, s.description.clone(), targets)
            })
            .collect();
        items.extend(loaded.instructions.iter().map(|(name, text)| {
            let first = text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("");
            let first = first.trim_start_matches('#').trim().to_string();
            item(
                ItemKind::Instructions,
                name,
                first,
                Loaded::aim(&m.instructions, name),
            )
        }));
        items.extend(m.mcp_servers.iter().map(|(name, s)| {
            item(
                ItemKind::McpServer,
                name,
                s.description().to_string(),
                s.targets(),
            )
        }));
        items.extend(m.hooks.iter().map(|(name, h)| {
            let on = h
                .matcher
                .as_deref()
                .map_or(String::new(), |p| format!(" ({p})"));
            let description = format!("{:?}{on}: {}", h.event, h.command);
            item(ItemKind::Hook, name, description, &h.targets)
        }));
        SetupView {
            items,
            harnesses: inventory::harnesses(home, installed),
            problems: loaded.problems,
        }
    }

    /// Aim one item at `targets`. An unreadable `setup.json` is left alone.
    pub fn set_targets(&self, kind: ItemKind, name: &str, targets: Targets) -> Result<(), String> {
        let mut manifest = self.read_manifest()?;
        let missing = |what: &str| format!("there is no {what} called {name}");
        match kind {
            ItemKind::Skill => {
                if !self
                    .dir
                    .join("skills")
                    .join(name)
                    .join("SKILL.md")
                    .is_file()
                {
                    return Err(missing("skill"));
                }
                manifest.skills.insert(name.to_string(), Aim { targets });
            }
            ItemKind::Instructions => {
                if !self
                    .dir
                    .join("instructions")
                    .join(format!("{name}.md"))
                    .is_file()
                {
                    return Err(missing("instructions file"));
                }
                manifest
                    .instructions
                    .insert(name.to_string(), Aim { targets });
            }
            ItemKind::McpServer => {
                *manifest
                    .mcp_servers
                    .get_mut(name)
                    .ok_or_else(|| missing("MCP server"))?
                    .targets_mut() = targets;
            }
            ItemKind::Hook => {
                manifest
                    .hooks
                    .get_mut(name)
                    .ok_or_else(|| missing("hook"))?
                    .targets = targets;
            }
        }
        write_manifest(&self.manifest_path(), &manifest)
    }
}

fn write_manifest(path: &Path, manifest: &Manifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("setup.json could not be written: {e}"))?;
    fs::write(path, text + "\n").map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// A name usable unquoted in a TOML dotted key (`-c mcp_servers.<name>.command=…`).
fn is_bare_key(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The `<name>.md` files in `dir`, by name, with their text.
fn read_instructions(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let name = file.strip_suffix(".md")?;
            if name.starts_with('.') {
                return None;
            }
            Some((name.to_string(), fs::read_to_string(entry.path()).ok()?))
        })
        .collect();
    files.sort();
    files
}

/// The skill folders in `dir` (following symlinks), by name. Hidden folders and
/// folders without a `SKILL.md` are not skills.
fn read_skills(dir: &Path) -> Vec<SkillInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut skills: Vec<SkillInfo> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let text = fs::read_to_string(entry.path().join("SKILL.md")).ok()?;
            Some(SkillInfo {
                description: front_matter_description(&text),
                name,
            })
        })
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// The `description` in a SKILL.md's front matter, a plain or block (`|`, `>`) value.
fn front_matter_description(text: &str) -> String {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    let front: Vec<&str> = lines.take_while(|l| l.trim() != "---").collect();
    let Some(at) = front.iter().position(|l| l.starts_with("description:")) else {
        return String::new();
    };
    let value = front[at]["description:".len()..].trim();
    let text = if value.is_empty() || value.starts_with('|') || value.starts_with('>') {
        front[at + 1..]
            .iter()
            .take_while(|l| l.starts_with(' ') || l.starts_with('\t') || l.is_empty())
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        value.to_string()
    };
    text.trim_matches(|c| c == '"' || c == '\'').to_string()
}
