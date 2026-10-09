# Harness hub research (2026-10-08)

The idea: Office Building becomes a hub for coding-agent harnesses. The backend is swappable (Claude Code, Codex, Gemini CLI, Antigravity, opencode, Cursor CLI), the way an old integrated development environment (IDE) swaps languages and plugins. Skills, hooks, Model Context Protocol (MCP) servers and plugins are set up once, and each item says which harnesses it applies to. A setup floor is where agents work on that setup and where the user sees everything installed.

Notes: [protocols.md](protocols.md) (structured protocols), [config.md](config.md) (per-harness config), [landscape.md](landscape.md) (other products, terms, release churn). Decision: OB-040 in [HUMAN_DECISIONS.md](../../../HUMAN_DECISIONS.md).

## Decided

- The building launches each tool's own, unmodified command-line interface (CLI) and layers its config on at launch. It never runs its own agent loop and never touches login tokens. Seen from the vendor's side, it is a terminal.
- Existing setups are discovered on first run and then watched for changes.
- Terminal by default, plus a structured protocol where the harness offers an official one.
- rulesync writes each harness's files wherever it can. Our own code covers per-item targeting and whatever rulesync lacks.
- Open source and public, never commercial.

## Terms

Checked at the source (https://code.claude.com/docs/en/legal-and-compliance):

- Anthropic allows "an end user signing in to the unmodified Claude Code binary with their own Claude subscription, including where a platform hosts Claude Code". Launch flags like `--settings` leave the binary unmodified.
- Third-party developers may not offer Claude.ai login, route requests through a user's plan credentials, or "collect, store, or intermediate Claude.ai credentials or session tokens".
- That rules out the Claude Agent SDK and the Claude adapter for the Agent Client Protocol (ACP) on a subscription login, because both are built on the SDK. Claude stays terminal-only.
- Products may say in plain text that they run Claude Code. The Claude Code and Anthropic names and logos must not appear in our product name, feature names or characters.
- Google: forum posts report mass bans for third-party use of Gemini CLI and Antigravity logins (secondary source). Running the official `agy` should be fine. Community ACP wrappers around it are not.

## Structured mode per harness

| Harness | Structured mode |
|---|---|
| Gemini CLI | Native ACP (`gemini --acp`) |
| opencode | Native ACP (`opencode acp`) |
| Cursor CLI | Native ACP (`agent acp`) |
| Codex | Codex app-server, official, signs in with a ChatGPT account (still experimental) |
| Claude Code | Terminal only |
| Antigravity | Terminal only, no official ACP |

For Rust there is the `agent-client-protocol` crate.

Untested idea: the building injects hooks at launch that report tool calls and permission waits back to it. Terminal sessions would then get typed status and keep their native interface.

## What ports across harnesses

- Skills: every harness reads `SKILL.md`. Codex, Gemini, opencode and Cursor also share `~/.agents/skills`, so a per-item choice must write to each harness's own folder, never the shared one.
- Instructions: one source renders to `AGENTS.md`, `CLAUDE.md` and `GEMINI.md`.
- MCP: the meaning maps cleanly. The file format and the key for a remote server differ per harness.
- Hooks: only before and after a tool runs, stop, and session start port. opencode has no shell hooks at all.
- Plugins: five different manifest formats. A plugin is unpacked into the parts above, or kept to its own harness.

Layering at launch is easiest in Claude Code (`--settings`, `--setting-sources`, `--mcp-config`, `--plugin-dir`, `--add-dir`) and opencode (environment variables). Codex has `-c` and profiles. Gemini and Cursor have partial flags. Antigravity has none, so its folders get a managed section.

## Landscape

Hosts (Zed, Emdash, Nimbalyst, Claude Squad) run several harnesses and leave their config alone. Config tools (rulesync, ruler, openskills, MCPM) write files from the command line and have no live view. rulesync comes closest: MIT licence, about 70 targets, hooks included. It picks whole features per tool, not single items. No product found joins live agents, one managed setup with per-item targeting, and agents working on that setup.

## Churn

Claude Code shipped about 100 releases between June and October 2026, opencode 96 in five months. Keeping adapters current is the main running cost, which is why rulesync does the bulk of the file writing. Our own adapters need a version check and golden-file tests.
