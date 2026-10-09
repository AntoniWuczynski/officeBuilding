<!-- Research notes, 2026-10-08. Sourced per line; UNVERIFIED marks claims not confirmed at a primary source. Synthesis: README.md. -->

# Harness config research (2026-10-08)

## 1. Claude Code (v2.1.294 local)
- Config home ~/.claude; CLAUDE_CONFIG_DIR relocates settings, session history, plugins (https://code.claude.com/docs/en/settings). ~/.claude.json (sign-in session, user/local MCP, per-project trust) is a separate file (same page). Whether ~/.claude.json moves with CLAUDE_CONFIG_DIR: UNVERIFIED. Only user/managed settings or the shell can set it (https://code.claude.com/docs/en/env-vars).
- Flags (local `claude --help`): --settings <file-or-json>, --setting-sources user,project,local, --mcp-config, --strict-mcp-config, --plugin-dir (repeatable, dir or .zip), --plugin-url, --add-dir (loads that dir's .claude/skills, commands, agents; https://code.claude.com/docs/en/skills), --agents <json>, --append-system-prompt[-file], --bare, --restricted. Env CLAUDE_CODE_PLUGIN_DIRS (v2.1.280+), CLAUDE_CODE_PLUGIN_CACHE_DIR (env-vars page).
- Settings precedence: managed > --settings > .claude/settings.local.json > .claude/settings.json > ~/.claude/settings.json (settings page).
- Skills: ~/.claude/skills, .claude/skills, plugin skills; follows agentskills.io plus extensions; ~/.agents/skills NOT documented (UNVERIFIED) (https://code.claude.com/docs/en/skills).
- Hooks: settings JSON `hooks`: event -> [{matcher, hooks:[{type: command|http|mcp_tool|prompt|agent, command, if}]}]; events PreToolUse, PostToolUse, UserPromptSubmit, Stop, SessionStart/End, SubagentStart/Stop, PreCompact, Notification etc (~30). exit 2 blocks (https://code.claude.com/docs/en/hooks). Also in plugin hooks/hooks.json and skill frontmatter.
- MCP: .mcp.json (`mcpServers`, type http|sse|ws|stdio, ${VAR:-default}); local+user scope in ~/.claude.json (https://code.claude.com/docs/en/mcp).
- Instructions: CLAUDE.md (+ .claude/rules).
- Sessions: under config dir `projects/<encoded-cwd>/*.jsonl` (settings page says session history lives in config dir; exact layout UNVERIFIED from docs, known from local fs).

## 2. Codex CLI (0.161.0 local)
- CODEX_HOME default ~/.codex: config.toml, auth.json (or keyring), history.jsonl (https://learn.chatgpt.com/docs/config-file/config-advanced, redirected from developers.openai.com/codex/config-advanced). Overriding CODEX_HOME => new auth.json/login needed unless copied (inference, UNVERIFIED).
- Flags (local --help): -c key=value (TOML, repeatable), -p/--profile (loads ~/.codex/<name>.config.toml; [profiles.x] tables dropped since 0.134), --enable/--disable feature, --add-dir, -C, -s, --oss. `codex plugin add|list|marketplace|remove`, `codex mcp add|list|get|remove|login`.
- Project .codex/config.toml loaded only if trusted; model_provider/profile/notify/otel ignored there.
- Skills: .agents/skills (cwd up to repo root), ~/.agents/skills, /etc/codex/skills; SKILL.md name+description; optional agents/openai.yaml (https://learn.chatgpt.com/docs/build-skills).
- Hooks: ~/.codex/hooks.json|config.toml, <repo>/.codex/hooks.json|config.toml, plugin hooks/hooks.json, requirements.toml. Same event->matcher->handlers JSON shape as Claude; events PreToolUse, PermissionRequest, PostToolUse, PreCompact, PostCompact, UserPromptSubmit, SubagentStop, Stop, SessionStart, SubagentStart, SessionEnd, Interrupt; exit 2 blocks; non-managed hooks need /hooks trust review (https://learn.chatgpt.com/docs/hooks).
- MCP: [mcp_servers.<name>] TOML in config.toml (command/args/env/url/bearer_token_env_var) - from third-party guides, official page not fetched (UNVERIFIED officially).
- Instructions: AGENTS.md; CODEX_HOME/AGENTS.override.md|AGENTS.md global; per-dir AGENTS.override.md > AGENTS.md > project_doc_fallback_filenames; 32KiB cap (https://learn.chatgpt.com/docs/agent-configuration/agents-md).
- Sessions: ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl (third-party; local ~/.codex/sessions exists) + sqlite DBs.

## 3. Gemini CLI
- ~/.gemini; GEMINI_CLI_HOME = root dir in which .gemini is created (https://geminicli.com/docs/reference/configuration/). Also GEMINI_CLI_SYSTEM_SETTINGS_PATH, GEMINI_CLI_SYSTEM_DEFAULTS_PATH, GEMINI_CLI_TRUSTED_FOLDERS_PATH.
- NOTE: for unpaid/Google One users Gemini CLI replaced by Antigravity CLI on 2026-06-18 (same page).
- Flags: -e/--extensions, --allowed-mcp-server-names, --include-directories, -r/--resume, --list-sessions, --approval-mode, --skip-trust (https://geminicli.com/docs/cli/cli-reference/). No per-launch settings-file flag documented.
- Settings: ~/.gemini/settings.json, .gemini/settings.json, system. Hooks in settings.json `hooks` (events BeforeTool, AfterTool, BeforeAgent, AfterAgent, BeforeModel, AfterModel, BeforeToolSelection, SessionStart/End, PreCompress, Notification; exit 2 blocks; JSON stdout; CLAUDE_PROJECT_DIR alias) (https://geminicli.com/docs/hooks/).
- MCP: settings.json `mcpServers` {command,args,env,cwd,url(SSE),httpUrl(HTTP),headers,trust,includeTools,excludeTools}.
- Skills: ~/.gemini/skills or ~/.agents/skills; .gemini/skills or .agents/skills (.agents wins) (https://geminicli.com/docs/cli/skills/).
- Extensions: ~/.gemini/extensions/<n>/gemini-extension.json (mcpServers, contextFileName, excludeTools, settings) + commands/*.toml, skills/, hooks/hooks.json, agents/, policies/ (https://geminicli.com/docs/extensions/reference/). `gemini extensions link <path>`.
- Context: GEMINI.md (context.fileName configurable e.g. add AGENTS.md).
- Sessions: ~/.gemini/tmp/<project_hash>/chats/ (https://geminicli.com/docs/cli/session-management/).

## 4. Google Antigravity (CLI `agy`)
- Paths conflict across sources. Official: settings ~/.gemini/antigravity-cli/settings.json (https://antigravity.google/docs/settings); global MCP ~/.gemini/config/mcp_config.json, workspace .agents/mcp_config.json (https://antigravity.google/docs/mcp); skills workspace .agents/skills, CLI global ~/.gemini/antigravity-cli/skills, IDE/2.0 global ~/.gemini/config/skills; ~/.agents/skills NOT mentioned (https://antigravity.google/docs/skills); hooks .agents/hooks.json + ~/.gemini/config/hooks.json + plugin hooks.json, events PreToolUse, PostToolUse, PreInvocation, PostInvocation, Stop; exit codes undocumented (https://antigravity.google/docs/hooks); rules AGENTS.md/GEMINI.md, .agents/rules/*.md (frontmatter trigger), ~/.gemini/{AGENTS,GEMINI}.md, ~/.gemini/config/rules, ~/.gemini/antigravity-cli/rules (https://antigravity.google/docs/rules); plugins: plugin.json (name, description only) + skills/ agents/ rules/ mcp_config.json hooks.json, installed to .agents/plugins, ~/.gemini/config/plugins, CLI staging ~/.gemini/antigravity-cli/plugins/<n> (https://antigravity.google/docs/plugins). `agy plugin import gemini` converts Gemini extensions (https://antigravity.google/docs/cli/gcli-migration/).
- Flags: -p, -c/--continue, --conversation <id>, --model, --agent, --output-format, --sandbox, --dangerously-skip-permissions (https://antigravity.google/docs/cli/headless). Env var for config home: none documented (UNVERIFIED). No per-launch settings/MCP/plugin-dir flag documented.
- Sessions: directory-scoped; disk location undocumented (UNVERIFIED).

## 5. opencode
- Global ~/.config/opencode/opencode.json; merge order: remote < global < OPENCODE_CONFIG < project opencode.json < .opencode dirs < OPENCODE_CONFIG_CONTENT (inline JSON) < managed (https://opencode.ai/docs/config/). OPENCODE_CONFIG_DIR = extra dir for agents/commands/modes/plugins. OPENCODE_PERMISSION inline JSON (https://opencode.ai/docs/cli/).
- MCP: `mcp` key ({type: local|remote, command, url, enabled}). Plugins: `plugin` array (npm) or .opencode/plugins, ~/.config/opencode/plugins JS/TS modules; hooks = event names (tool.execute.before/after, session.*, shell.env...) (https://opencode.ai/docs/plugins/).
- Skills: .opencode/skills, .claude/skills, .agents/skills, ~/.config/opencode/skills, ~/.claude/skills, ~/.agents/skills (https://opencode.ai/docs/skills/).
- Instructions: AGENTS.md (fallback CLAUDE.md), ~/.config/opencode/AGENTS.md (fallback ~/.claude/CLAUDE.md); `instructions` array of paths/globs (can point at .cursor/rules) (https://opencode.ai/docs/rules/). OPENCODE_DISABLE_CLAUDE_CODE[_PROMPT|_SKILLS].
- Sessions: SQLite ~/.local/share/opencode/opencode.db (since 1.2), `opencode db path`; OPENCODE_DB override; third-party sources (https://deepwiki.com/sst/opencode/2.9-storage-and-database). Resume: run -c / -s <id>, export.

## 6. Cursor CLI (cursor-agent / `agent`)
- ~/.cursor/cli-config.json (global; project .cursor/cli.json permissions only); CURSOR_CONFIG_DIR, XDG_CONFIG_HOME (https://cursor.com/docs/cli/reference/configuration). Auth: CURSOR_API_KEY / --api-key (https://cursor.com/docs/cli/reference/parameters).
- Flags: --plugin-dir (repeatable), --workspace, --approve-mcps, --resume [id], --continue, --mode, -f/--force. No --settings/--mcp-config flag documented.
- MCP: .cursor/mcp.json, ~/.cursor/mcp.json (`mcpServers`, ${env:NAME}) (https://cursor.com/docs/context/mcp). Hooks: ~/.cursor/hooks.json, .cursor/hooks.json, `{version:1, hooks:{camelCaseEvent:[{command,...}]}}`, exit 2 blocks, many events; CLI support of events beyond workspaceOpen unconfirmed (https://cursor.com/docs/hooks).
- Skills: .agents/skills, .cursor/skills, ~/.agents/skills, ~/.cursor/skills plus .claude/.codex skill dirs (https://cursor.com/docs/context/skills). Rules: .cursor/rules/*.mdc, AGENTS.md; User Rules are in-app only (https://cursor.com/docs/context/rules).
- Sessions: `agent ls`, `agent resume`, --resume <id>; disk location UNVERIFIED.

## 7. Additions
- Claude plugin: .claude-plugin/plugin.json (name required) + skills/ commands/ agents/ hooks/hooks.json .mcp.json .lsp.json bin/ settings.json; ${CLAUDE_PLUGIN_ROOT} (https://code.claude.com/docs/en/plugins-reference). A plugin-level CLAUDE.md is NOT loaded.
- Codex config reference: mcp_servers.<id> {command,args,url,env,enabled_tools,disabled_tools,startup_timeout_sec,tool_timeout_sec}; plugins.<name@marketplace>.enabled; sqlite_home; history.persistence (https://learn.chatgpt.com/docs/config-file/config-reference). NB developers.openai.com 308-redirects to learn.chatgpt.com.
- Cursor plugins: .cursor-plugin/plugin.json (rules, skills, agents, commands, MCP, hooks); "Agent Plugin" open standard plugin.json (skills + MCP only); local at ~/.cursor/plugins/local (https://cursor.com/docs/plugins).
- Local verification: ~/.codex/{config.toml,hooks.json,auth.json,sessions/2026/..,plugins,*.sqlite}; ~/.claude/{projects,sessions,skills,plugins,settings.json}; ~/.claude.json exists in $HOME; no ~/.agents dir except skills/.
