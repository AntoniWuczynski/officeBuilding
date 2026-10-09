//! What a tool is launched with to take the building's items: environment
//! variables and options. Generated folders live in the app's data directory
//! and link back to the library, so a running agent sees a skill edited on the
//! setup floor. Each mechanism was checked by hand (docs/harness-hub-plan.md):
//!
//! * Claude Code: a plugin folder (`.claude-plugin/plugin.json`, `skills/<name>`
//!   links, `.mcp.json`, `hooks/hooks.json`) passed with `--plugin-dir`, and
//!   instructions with `--append-system-prompt-file`. Its skills show up as
//!   `office-building:<name>`.
//! * Antigravity: a folder with `.agents/skills/<name>` links and an `AGENTS.md`,
//!   passed with `--add-dir`.
//! * Codex: MCP servers as `-c mcp_servers.<name>.<field>=<TOML value>`. Skills,
//!   instructions and hooks through an overlay `CODEX_HOME`: every entry of
//!   `~/.codex` linked back to the real one (sessions, login and databases stay
//!   there), except `skills/` (the real skills plus the building's), and
//!   `AGENTS.md` and `hooks.json` (the real ones plus the building's).
//! * Cursor: a plugin folder (`.cursor-plugin/plugin.json`, `skills/<name>`
//!   links) passed with `--plugin-dir`; it carries skills only.
//! * opencode: `OPENCODE_CONFIG_DIR` (added to the user's own config) holding
//!   `skills/<name>` links, and `OPENCODE_CONFIG_CONTENT` with the MCP servers
//!   and instruction files.

use super::files::{entries, generated_or_link, link, sync_links, write_file, write_or_remove};
use super::{Hook, HookEvent, ItemKind, Library, McpServer, Targets, support};
use crate::feed::{self, Feed};
use crate::model::ToolKind;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const PLUGIN_MANIFEST: &str = r#"{
  "name": "office-building",
  "version": "0.1.0",
  "description": "Skills, MCP servers and hooks from Office Building's setup floor"
}
"#;

/// An option and its value; the value is quoted when the command is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    pub flag: &'static str,
    pub value: String,
}

/// What a launch adds: environment variables set for the tool, and options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Launch {
    pub env: Vec<(&'static str, String)>,
    pub opts: Vec<Opt>,
}

impl Launch {
    /// `env NAME='value' … ` before the binary, or nothing.
    #[must_use]
    pub fn prefix(&self, quote: &dyn Fn(&str) -> String) -> String {
        if self.env.is_empty() {
            return String::new();
        }
        let mut out = String::from("env");
        for (name, value) in &self.env {
            for part in [" ", name, "=", &quote(value)] {
                out.push_str(part);
            }
        }
        out.push(' ');
        out
    }

    /// The options, each led by a space, after the binary.
    #[must_use]
    pub fn args(&self, quote: &dyn Fn(&str) -> String) -> String {
        let mut out = String::new();
        for o in &self.opts {
            for part in [" ", o.flag, " ", &quote(&o.value)] {
                out.push_str(part);
            }
        }
        out
    }

    fn opt(&mut self, flag: &'static str, path: &Path) {
        self.opts.push(Opt {
            flag,
            value: path.to_string_lossy().into_owned(),
        });
    }
}

/// The items aimed at one tool, among the kinds it can take.
struct Wanted<'a> {
    skills: Vec<&'a str>,
    /// (name, text)
    instructions: Vec<(&'a str, &'a str)>,
    servers: BTreeMap<&'a str, &'a McpServer>,
    /// The library's hooks aimed at this tool, then the status feed's.
    hooks: Vec<Hook>,
}

impl Wanted<'_> {
    fn is_empty(&self) -> bool {
        self.skills.is_empty()
            && self.instructions.is_empty()
            && self.servers.is_empty()
            && self.hooks.is_empty()
    }

    /// The instructions as one text, or none.
    fn instructions_text(&self) -> Option<String> {
        if self.instructions.is_empty() {
            return None;
        }
        let parts: Vec<&str> = self
            .instructions
            .iter()
            .map(|(_, text)| text.trim())
            .collect();
        Some(parts.join("\n\n") + "\n")
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeMcpFile<'a> {
    mcp_servers: BTreeMap<&'a str, ClaudeMcp<'a>>,
}

/// A command server as the building writes it, or `{ "type": "http", "url", "headers" }`
/// (<https://code.claude.com/docs/en/plugins-reference>).
#[derive(Serialize)]
#[serde(untagged)]
enum ClaudeMcp<'a> {
    Command {
        command: &'a str,
        args: &'a [String],
        env: &'a BTreeMap<String, String>,
    },
    Http {
        #[serde(rename = "type")]
        kind: &'static str,
        url: &'a str,
        headers: &'a BTreeMap<String, String>,
    },
}

impl<'a> From<&'a McpServer> for ClaudeMcp<'a> {
    fn from(server: &'a McpServer) -> Self {
        match server {
            McpServer::Command(s) => Self::Command {
                command: &s.command,
                args: &s.args,
                env: &s.env,
            },
            McpServer::Http(s) => Self::Http {
                kind: "http",
                url: &s.url,
                headers: &s.headers,
            },
        }
    }
}

/// `{ "matcher": …, "hooks": [{ "type": "command", "command": … }] }`, as
/// Claude Code and Codex both read hooks.
#[derive(Serialize)]
struct HookGroup<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    matcher: Option<&'a str>,
    hooks: [HookCommand<'a>; 1],
}

#[derive(Serialize)]
struct HookCommand<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    command: &'a str,
}

#[derive(Serialize)]
struct HooksFile<'a> {
    hooks: BTreeMap<HookEvent, Vec<HookGroup<'a>>>,
}

fn hook_groups(hooks: &[Hook]) -> BTreeMap<HookEvent, Vec<HookGroup<'_>>> {
    let mut out: BTreeMap<HookEvent, Vec<HookGroup<'_>>> = BTreeMap::new();
    for h in hooks {
        out.entry(h.event).or_default().push(HookGroup {
            matcher: h.matcher.as_deref(),
            hooks: [HookCommand {
                kind: "command",
                command: &h.command,
            }],
        });
    }
    out
}

/// Codex's own `hooks.json`, kept as it is apart from the building's additions.
#[derive(Deserialize, Default)]
struct CodexHooks {
    #[serde(default)]
    hooks: BTreeMap<HookEventName, Vec<Box<RawValue>>>,
}

/// An event name in Codex's file: one the building knows, or any other kept as written.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
enum HookEventName {
    Known(HookEvent),
    Other(String),
}

#[derive(Serialize)]
struct CodexHooksOut {
    hooks: BTreeMap<HookEventName, Vec<Box<RawValue>>>,
}

/// opencode's config, as `OPENCODE_CONFIG_CONTENT` (<https://opencode.ai/docs/mcp-servers/>,
/// <https://opencode.ai/docs/rules/>).
#[derive(Serialize)]
struct OpencodeConfig<'a> {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    mcp: BTreeMap<&'a str, OpencodeMcp<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    instructions: Vec<String>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum OpencodeMcp<'a> {
    Local {
        command: Vec<&'a str>,
        environment: &'a BTreeMap<String, String>,
    },
    Remote {
        url: &'a str,
        headers: &'a BTreeMap<String, String>,
    },
}

impl<'a> From<&'a McpServer> for OpencodeMcp<'a> {
    fn from(server: &'a McpServer) -> Self {
        match server {
            McpServer::Command(s) => {
                let mut command = vec![s.command.as_str()];
                command.extend(s.args.iter().map(String::as_str));
                Self::Local {
                    command,
                    environment: &s.env,
                }
            }
            McpServer::Http(s) => Self::Remote {
                url: &s.url,
                headers: &s.headers,
            },
        }
    }
}

fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| format!("the building's setup could not be written: {e}"))
}

impl Library {
    /// What `tool` is launched with to take the items aimed at it, and the
    /// status feed's hooks when there is a `feed`. Empty when there is nothing.
    pub fn launch(
        &self,
        tool: ToolKind,
        home: &Path,
        feed: Option<&Feed>,
    ) -> Result<Launch, String> {
        let loaded = self.load();
        let can = |kind| support(tool, kind).is_supported();
        let m = &loaded.manifest;
        let wanted = Wanted {
            skills: if can(ItemKind::Skill) {
                loaded.skills_for(tool)
            } else {
                Vec::new()
            },
            instructions: if can(ItemKind::Instructions) {
                loaded.instructions_for(tool)
            } else {
                Vec::new()
            },
            servers: m
                .mcp_servers
                .iter()
                .filter(|(_, s)| can(ItemKind::McpServer) && s.targets().includes(tool))
                .map(|(name, s)| (name.as_str(), s))
                .collect(),
            hooks: m
                .hooks
                .values()
                .filter(|h| {
                    can(ItemKind::Hook) && h.targets.includes(tool) && h.event.exists_in(tool)
                })
                .cloned()
                .chain(feed.into_iter().flat_map(|f| {
                    feed::events(tool).iter().map(|&event| Hook {
                        event,
                        matcher: None,
                        command: f.command(event),
                        targets: Targets::All,
                    })
                }))
                .collect(),
        };
        if wanted.is_empty() {
            return Ok(Launch::default());
        }
        let mut launch = Launch::default();
        match tool {
            ToolKind::ClaudeCode => self.claude(&wanted, &mut launch)?,
            ToolKind::Antigravity => {
                let dir = self.build.join("antigravity");
                self.link_skills(&dir.join(".agents/skills"), &wanted.skills)?;
                write_or_remove(
                    &dir.join("AGENTS.md"),
                    wanted.instructions_text().as_deref(),
                )?;
                launch.opt("--add-dir", &dir);
            }
            ToolKind::Codex => {
                let overlay = !(wanted.skills.is_empty()
                    && wanted.instructions.is_empty()
                    && wanted.hooks.is_empty());
                if overlay {
                    let dir = self.codex_home(home, &wanted)?;
                    launch
                        .env
                        .push(("CODEX_HOME", dir.to_string_lossy().into_owned()));
                }
                launch.opts.extend(
                    wanted
                        .servers
                        .iter()
                        .flat_map(|(name, s)| codex_overrides(name, s))
                        .map(|value| Opt { flag: "-c", value }),
                );
            }
            ToolKind::Opencode => self.opencode(&wanted, &mut launch)?,
            ToolKind::Cursor => {
                let dir = self.build.join("cursor");
                write_file(&dir.join(".cursor-plugin/plugin.json"), PLUGIN_MANIFEST)?;
                self.link_skills(&dir.join("skills"), &wanted.skills)?;
                launch.opt("--plugin-dir", &dir);
            }
            ToolKind::Unknown => {}
        }
        Ok(launch)
    }

    /// The plugin folder Claude Code is pointed at, and its instructions file.
    fn claude(&self, wanted: &Wanted<'_>, launch: &mut Launch) -> Result<(), String> {
        let dir = self.build.join("claude-code");
        if !(wanted.skills.is_empty() && wanted.servers.is_empty() && wanted.hooks.is_empty()) {
            write_file(&dir.join(".claude-plugin/plugin.json"), PLUGIN_MANIFEST)?;
            self.link_skills(&dir.join("skills"), &wanted.skills)?;
            let servers = ClaudeMcpFile {
                mcp_servers: wanted
                    .servers
                    .iter()
                    .map(|(&name, &s)| (name, ClaudeMcp::from(s)))
                    .collect(),
            };
            let servers = (!wanted.servers.is_empty())
                .then(|| json(&servers))
                .transpose()?;
            write_or_remove(&dir.join(".mcp.json"), servers.as_deref())?;
            let hooks = HooksFile {
                hooks: hook_groups(&wanted.hooks),
            };
            let hooks = (!wanted.hooks.is_empty())
                .then(|| json(&hooks))
                .transpose()?;
            write_or_remove(&dir.join("hooks/hooks.json"), hooks.as_deref())?;
            launch.opt("--plugin-dir", &dir);
        }
        if let Some(text) = wanted.instructions_text() {
            let file = self.build.join("claude-code-instructions.md");
            write_file(&file, &text)?;
            launch.opt("--append-system-prompt-file", &file);
        }
        Ok(())
    }

    /// opencode's extra config folder (skills) and inline config (servers, instructions).
    fn opencode(&self, wanted: &Wanted<'_>, launch: &mut Launch) -> Result<(), String> {
        if !wanted.skills.is_empty() {
            let dir = self.build.join("opencode");
            self.link_skills(&dir.join("skills"), &wanted.skills)?;
            launch
                .env
                .push(("OPENCODE_CONFIG_DIR", dir.to_string_lossy().into_owned()));
        }
        let config = OpencodeConfig {
            mcp: wanted
                .servers
                .iter()
                .map(|(&name, &s)| (name, OpencodeMcp::from(s)))
                .collect(),
            instructions: wanted
                .instructions
                .iter()
                .map(|(name, _)| {
                    let file = self.dir.join("instructions").join(format!("{name}.md"));
                    file.to_string_lossy().into_owned()
                })
                .collect(),
        };
        if !(config.mcp.is_empty() && config.instructions.is_empty()) {
            let text = serde_json::to_string(&config)
                .map_err(|e| format!("the building's setup could not be written: {e}"))?;
            launch.env.push(("OPENCODE_CONFIG_CONTENT", text));
        }
        Ok(())
    }

    fn link_skills(&self, dir: &Path, names: &[&str]) -> Result<(), String> {
        let wanted: Vec<(String, PathBuf)> = names
            .iter()
            .map(|n| ((*n).to_string(), self.dir.join("skills").join(n)))
            .collect();
        sync_links(dir, &wanted)
    }

    /// The overlay `CODEX_HOME`: the real `~/.codex` entry by entry, with a
    /// `skills/` that also holds the building's (a skill the user already has
    /// under the same name keeps theirs), and `AGENTS.md` and `hooks.json`
    /// that add the building's to the user's own.
    fn codex_home(&self, home: &Path, wanted: &Wanted<'_>) -> Result<PathBuf, String> {
        let real = home.join(".codex");
        let dir = self.build.join("codex-home");
        fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        for (name, target) in entries(&real) {
            if !matches!(name.as_str(), "skills" | "AGENTS.md" | "hooks.json") {
                link(&dir.join(&name), &target)?;
            }
        }
        let mut skills = entries(&real.join("skills"));
        for name in &wanted.skills {
            if !skills.iter().any(|(n, _)| n == name) {
                skills.push(((*name).to_string(), self.dir.join("skills").join(name)));
            }
        }
        sync_links(&dir.join("skills"), &skills)?;

        let agents = real.join("AGENTS.md");
        let instructions = wanted.instructions_text().map(|ours| {
            let theirs = fs::read_to_string(&agents).unwrap_or_default();
            if theirs.trim().is_empty() {
                ours
            } else {
                format!("{}\n\n{ours}", theirs.trim_end())
            }
        });
        generated_or_link(&dir.join("AGENTS.md"), instructions.as_deref(), &agents)?;

        let hooks_path = real.join("hooks.json");
        let hooks = if wanted.hooks.is_empty() {
            None
        } else {
            let mut theirs: CodexHooks = fs::read_to_string(&hooks_path)
                .ok()
                .map(|text| serde_json::from_str(&text))
                .transpose()
                .map_err(|e| format!("{} could not be read: {e}", hooks_path.display()))?
                .unwrap_or_default();
            for (event, groups) in hook_groups(&wanted.hooks) {
                for group in groups {
                    let raw = serde_json::value::to_raw_value(&group)
                        .map_err(|e| format!("a hook could not be written: {e}"))?;
                    theirs
                        .hooks
                        .entry(HookEventName::Known(event))
                        .or_default()
                        .push(raw);
                }
            }
            Some(json(&CodexHooksOut {
                hooks: theirs.hooks,
            })?)
        };
        generated_or_link(&dir.join("hooks.json"), hooks.as_deref(), &hooks_path)?;
        Ok(dir)
    }
}

/// `mcp_servers.<name>.command="npx"` and friends, values in TOML.
fn codex_overrides(name: &str, server: &McpServer) -> Vec<String> {
    let string = |v: &str| toml::Value::String(v.to_string());
    let table = |m: &BTreeMap<String, String>| {
        toml::Value::Table(m.iter().map(|(k, v)| (k.clone(), string(v))).collect())
    };
    let mut out = Vec::new();
    match server {
        McpServer::Command(m) => {
            out.push(format!("mcp_servers.{name}.command={}", string(&m.command)));
            if !m.args.is_empty() {
                let args = m.args.iter().map(|a| string(a)).collect();
                out.push(format!(
                    "mcp_servers.{name}.args={}",
                    toml::Value::Array(args)
                ));
            }
            if !m.env.is_empty() {
                out.push(format!("mcp_servers.{name}.env={}", table(&m.env)));
            }
        }
        McpServer::Http(m) => {
            out.push(format!("mcp_servers.{name}.url={}", string(&m.url)));
            if !m.headers.is_empty() {
                out.push(format!(
                    "mcp_servers.{name}.http_headers={}",
                    table(&m.headers)
                ));
            }
        }
    }
    out
}
