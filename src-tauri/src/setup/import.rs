//! Copy one of a tool's own skills or MCP servers into the library, so the
//! building can hand it to the other tools. The tool's own files are only read.
//! The copy goes to every tool that can take it except the one it came from,
//! which already has it.

use super::{
    CommandServer, HttpServer, ItemKind, Library, McpServer, Targets, inventory, support,
    write_manifest,
};
use crate::model::ToolKind;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// A server as Claude Code, Cursor and Antigravity write it: a command, or a
/// `url` with `headers` (Claude Code's `type` is `http`, `sse` or `ws`).
#[derive(Deserialize)]
struct JsonServer {
    #[serde(rename = "type")]
    kind: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
struct McpServers {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: BTreeMap<String, JsonServer>,
}

/// A server in Codex's `config.toml`: a command, or a streamable HTTP `url`
/// (<https://learn.chatgpt.com/docs/config-file/config-reference>).
#[derive(Deserialize)]
struct CodexServer {
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    url: Option<String>,
    #[serde(default)]
    http_headers: BTreeMap<String, String>,
    bearer_token_env_var: Option<String>,
    #[serde(default)]
    env_http_headers: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
struct CodexConfig {
    #[serde(default)]
    mcp_servers: BTreeMap<String, CodexServer>,
}

/// opencode's `{ "type": "local", "command": [cmd, ...args], "environment": {} }`
/// or `{ "type": "remote", "url": …, "headers": {} }`
/// (<https://opencode.ai/docs/mcp-servers/>).
#[derive(Deserialize)]
struct OpencodeServer {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
struct OpencodeConfig {
    #[serde(default)]
    mcp: BTreeMap<String, OpencodeServer>,
}

fn read<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn command(command: String, args: Vec<String>, env: BTreeMap<String, String>) -> McpServer {
    McpServer::Command(CommandServer {
        command,
        args,
        env,
        targets: Targets::All,
    })
}

fn http(url: String, headers: BTreeMap<String, String>) -> McpServer {
    McpServer::Http(HttpServer {
        url,
        headers,
        targets: Targets::All,
    })
}

impl JsonServer {
    fn into_server(self) -> Option<McpServer> {
        match (self.command, self.url, self.kind.as_deref()) {
            (Some(c), _, _) => Some(command(c, self.args, self.env)),
            (None, Some(url), None | Some("http")) => Some(http(url, self.headers)),
            _ => None,
        }
    }
}

impl CodexServer {
    fn into_server(self) -> Option<McpServer> {
        let fixed_auth = self.bearer_token_env_var.is_none() && self.env_http_headers.is_empty();
        match (self.command, self.url) {
            (Some(c), _) => Some(command(c, self.args, self.env)),
            (None, Some(url)) if fixed_auth => Some(http(url, self.http_headers)),
            _ => None,
        }
    }
}

impl OpencodeServer {
    fn into_server(self) -> Option<McpServer> {
        if self.kind.as_deref() == Some("remote") {
            return self.url.map(|url| http(url, self.headers));
        }
        let mut words = self.command.into_iter();
        words
            .next()
            .map(|c| command(c, words.collect(), self.environment))
    }
}

/// `name` as `tool` runs it, or why it cannot be copied.
fn tool_server(tool: ToolKind, home: &Path, name: &str) -> Result<McpServer, String> {
    let json = |file: &str| {
        let mut all: McpServers = read(&home.join(file));
        all.mcp_servers.remove(name).map(JsonServer::into_server)
    };
    let found = match tool {
        ToolKind::ClaudeCode => json(".claude.json"),
        ToolKind::Cursor => json(".cursor/mcp.json"),
        ToolKind::Antigravity => json(".gemini/config/mcp_config.json"),
        ToolKind::Codex => {
            let mut all: CodexConfig = fs::read_to_string(home.join(".codex/config.toml"))
                .ok()
                .and_then(|text| toml::from_str(&text).ok())
                .unwrap_or_default();
            all.mcp_servers.remove(name).map(CodexServer::into_server)
        }
        ToolKind::Opencode => {
            let dir = home.join(".config/opencode");
            let mut all: OpencodeConfig = ["opencode.json", "opencode.jsonc"]
                .iter()
                .find_map(|f| fs::read_to_string(dir.join(f)).ok())
                .and_then(|text| serde_json::from_str(&inventory::strip_json_comments(&text)).ok())
                .unwrap_or_default();
            all.mcp.remove(name).map(OpencodeServer::into_server)
        }
        ToolKind::Unknown => None,
    };
    match found {
        None => Err(format!("{name} is not one of that tool's MCP servers")),
        Some(None) => Err(format!(
            "{name} cannot be copied yet: the building takes servers started by a command, or reached over streamable HTTP with fixed headers"
        )),
        Some(Some(server)) => Ok(server),
    }
}

/// A tool's name as `setup.json` writes it, for naming copies.
fn slug(tool: ToolKind) -> &'static str {
    match tool {
        ToolKind::ClaudeCode => "claude-code",
        ToolKind::Codex => "codex",
        ToolKind::Antigravity => "antigravity",
        ToolKind::Opencode => "opencode",
        ToolKind::Cursor => "cursor",
        ToolKind::Unknown => "unknown",
    }
}

/// Every tool that can take `kind`, but not `from`.
fn others(from: ToolKind, kind: ItemKind) -> Targets {
    let tools = [
        ToolKind::ClaudeCode,
        ToolKind::Codex,
        ToolKind::Antigravity,
        ToolKind::Opencode,
        ToolKind::Cursor,
    ]
    .into_iter()
    .filter(|&t| t != from && support(t, kind).is_supported())
    .collect();
    Targets::Only { tools }
}

impl Library {
    pub fn import(
        &self,
        home: &Path,
        from: ToolKind,
        kind: ItemKind,
        name: &str,
    ) -> Result<(), String> {
        if name.is_empty() || name.contains('/') || name.starts_with('.') {
            return Err(format!("{name} is not a name that can be copied"));
        }
        let mut manifest = self.read_manifest()?;
        let targets = others(from, kind);
        match kind {
            ItemKind::Skill => {
                let to = self.dir.join("skills").join(name);
                if to.exists() {
                    return Err(format!("the building already has a skill called {name}"));
                }
                let source = inventory::skill_dirs(from, home)
                    .into_iter()
                    .map(|d| d.join(name))
                    .find(|d| d.join("SKILL.md").is_file())
                    .ok_or_else(|| format!("{name} is not one of that tool's skills"))?;
                copy_dir(&source, &to)?;
                manifest
                    .skills
                    .insert(name.to_string(), super::Aim { targets });
            }
            ItemKind::Instructions => {
                let file = inventory::instructions_file(from, home)
                    .filter(|f| f.file_name().is_some_and(|n| n.to_string_lossy() == name))
                    .ok_or_else(|| format!("{name} is not that tool's instructions file"))?;
                let text = std::fs::read_to_string(&file)
                    .map_err(|e| format!("could not read {}: {e}", file.display()))?;
                let ours = format!("{}-instructions", slug(from));
                let to = self.dir.join("instructions").join(format!("{ours}.md"));
                if to.exists() {
                    return Err(format!(
                        "the building already has instructions called {ours}"
                    ));
                }
                super::files::write_file(&to, &text)?;
                manifest.instructions.insert(ours, super::Aim { targets });
            }
            ItemKind::Hook => {
                let hook = inventory::hooks_of(from, home)
                    .into_iter()
                    .find(|h| h.id == name)
                    .ok_or_else(|| format!("{name} is not one of that tool's hooks"))?;
                let event = super::HookEvent::named(&hook.event).ok_or_else(|| {
                    format!(
                        "{} is not an event the building can hand to other tools",
                        hook.event
                    )
                })?;
                let ours = format!("{}-{}", slug(from), name.replace(':', "-").to_lowercase());
                if manifest.hooks.contains_key(&ours) {
                    return Err(format!("the building already has a hook called {ours}"));
                }
                let copied = super::Hook {
                    event,
                    matcher: hook.matcher,
                    command: hook.command,
                    targets,
                };
                manifest.hooks.insert(ours, copied);
            }
            ItemKind::McpServer => {
                if !super::is_bare_key(name) {
                    return Err(format!(
                        "{name} cannot be copied: use only letters, digits, - and _ in its name"
                    ));
                }
                if manifest.mcp_servers.contains_key(name) {
                    return Err(format!(
                        "the building already has an MCP server called {name}"
                    ));
                }
                let mut server = tool_server(from, home, name)?;
                *server.targets_mut() = targets;
                manifest.mcp_servers.insert(name.to_string(), server);
            }
        }
        write_manifest(&self.manifest_path(), &manifest)
    }
}

/// Copy a folder's contents, following links, so the copy owns its files.
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("could not create {}: {e}", to.display()))?;
    let entries =
        fs::read_dir(from).map_err(|e| format!("could not read {}: {e}", from.display()))?;
    for entry in entries.flatten() {
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if source.is_dir() {
            copy_dir(&source, &target)?;
        } else {
            fs::copy(&source, &target)
                .map_err(|e| format!("could not copy {}: {e}", source.display()))?;
        }
    }
    Ok(())
}
