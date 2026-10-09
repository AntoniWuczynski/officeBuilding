# Harness hub (OB-041, OB-042)

Decision: OB-040 in [HUMAN_DECISIONS.md](../HUMAN_DECISIONS.md). Research: [research/harness-hub/](research/harness-hub/README.md).

## What reaches each tool, and how

The building's setup applies only to the agents it launches. Nothing is written into a tool's own config folders: everything goes through launch options, environment variables or generated folders in the app's data directory. Each route below was checked by hand on 2026-10-08 with a probe item and the real tool.

| Tool | Skills | Instructions | MCP servers | Hooks |
|---|---|---|---|---|
| Claude Code | plugin folder, `--plugin-dir` | `--append-system-prompt-file` | plugin `.mcp.json` | plugin `hooks/hooks.json` |
| Codex | overlay `CODEX_HOME` `skills/` | overlay `AGENTS.md` (theirs plus ours) | `-c mcp_servers.<name>.…` | overlay `hooks.json` (theirs plus ours), after the human trusts them once with `/hooks` |
| Antigravity | `--add-dir`, `.agents/skills` | `--add-dir`, `AGENTS.md` | no launch option | no launch option |
| opencode | `OPENCODE_CONFIG_DIR` `skills/` | `OPENCODE_CONFIG_CONTENT` `instructions` | `OPENCODE_CONFIG_CONTENT` `mcp` | its hooks are code plugins |
| Cursor | plugin folder, `--plugin-dir` | `--plugin-dir` rules did not apply | `--plugin-dir` servers did not load | no launch option |

- **The overlay `CODEX_HOME`** links every entry of `~/.codex` back to the real one, so the login, sessions and databases stay where they are. A test run left its session in the real `~/.codex/sessions` and added no files beside the links. It is rebuilt at every launch.
- **rulesync was not used.** It writes only into the project it runs in, targets whole tools rather than single items, and needs a global install. Claude Code and Codex share one hook format. The tools whose formats differ have no launch option for hooks, so there was nothing for it to translate (A Human chose "rulesync for whatever works": nothing did).

## Built

- **Setup floor.** A plant room in the basement, below the lobby. Its folder (`Setup/` in the app's data directory) is the library:
  - `skills/<name>/SKILL.md`
  - `instructions/<name>.md`
  - `setup.json`, for MCP servers, hooks and every item's targets
  It goes up on first run. Its toolbox opens the Setup panel.
- **Setup panel.**
  - A per-item tool picker, with "All tools".
  - A tool that cannot take an item kind is greyed out, with the reason. A note covers Codex's hook trust.
  - A read-only list of what each tool already has (skills, MCP servers, hooks, plugins), with "Copy in" for skills and MCP servers (stdio, or streamable HTTP with fixed headers), and "Copy all in" per tool. A copy is aimed at the other tools.
  - It refreshes by itself when a watched file changes.
- **Hook status feed.** Agents the app launches (Claude Code, and Codex after trust) drop each hook event into a folder. A refresh lets the newest event set the desk's state, so a permission prompt shows as a raised hand. Checked with a real Claude Code run.
- **Structured desks.** A hire option, "Without a terminal", for Codex (app-server protocol) and for opencode and Cursor (Agent Client Protocol). The desk opens a conversation panel instead of a terminal: the transcript, tool calls, approvals with their choices, and a box for the next message. Codex's own session file for the same conversation gets no second desk.
- **Second pass (OB-042).**
  - Model lists for opencode and Cursor.
  - Copy in for instructions files and single hooks.
  - Resuming a structured desk from the sign-out sheet, with its conversation replayed (Codex `thread/resume`, ACP `session/load`).
  - Approval fields for questions and forms, including Cursor's `ask_question` and `create_plan`.

## Later

- Antigravity's MCP servers and hooks, and Cursor's MCP servers, hooks and instructions, once the tools offer a launch option (OB-043).
- Keeping structured desks across app restarts.
