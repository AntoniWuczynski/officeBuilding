# HUMAN DECISIONS — calls for A Human

A section with no status on its heading is open and shows in Office Building's
Decisions queue for this floor. Answering it in the app marks the heading
`DECIDED <date>` and records the answer here. Agent work lives in
[TODO.md](TODO.md), the human's actions in [HUMAN_TODO.md](HUMAN_TODO.md).

## OB-001 — Base the backend on CCC or build our own? — DECIDED 2026-09-29: our own Rust backend

CCC (claude-command-center) moved to a revocable, non-commercial,
source-available licence on 2026-07-28. Office Building will be public and
never commercial. We port logic only from CCC v5.14.0, the last MIT release,
and keep its notice (THIRD_PARTY_NOTICES.md). Evidence:
`docs/research/ccc-evaluation.md`. Supersedes the 2026-09-28 call to base it
on CCC.

## OB-002 — Where do Decisions and TODOs come from? — DECIDED 2026-09-29: the repo's own files

Decisions read from FOUNDER_DECISIONS.md or HUMAN_DECISIONS.md (plus the
agents' DECISIONS.md log), human TODOs from FOUNDER_TODO.md or HUMAN_TODO.md,
agent TODOs from TODO.md, each falling back to the app's own store when a repo
has no such file.

## OB-040 — Should the building become a hub for agent harnesses? — DECIDED 2026-10-08: yes, a hub that launches each official CLI

Decided by A Human on 2026-10-08:

- Office Building becomes a hub where the backend (Claude Code, Codex, Antigravity, opencode, Cursor CLI) is swappable. Gemini CLI is deprecated, and Antigravity replaces it. Skills, hooks, MCP servers and plugins are set up once, and each item picks the harnesses it applies to, with an option for all of them.
- A setup floor is where agents work on that setup.
- The building runs each tool's own unmodified CLI, so to the vendor it is no different from a terminal. It never runs its own agent loop and never touches login tokens.
- Existing setups are discovered on first run and then watched.
- Terminal by default, plus a structured protocol where the harness has an official one (hybrid).
- rulesync does whatever it can, and our own code does the rest.
- The building's setup applies only inside the app (A Human, 2026-10-08). It never writes into the real `~/.claude`, `~/.codex` or a project's harness folders. rulesync writes into a staging folder the building owns, which reaches each tool only at launch.
- Open source and public, never commercial.
- Follow-up calls (A Human, 2026-10-08):
  - The setup floor looks different from the rest. It is a plant room in the basement, below the lobby.
  - Install opencode and Cursor so their support can be checked against the real tools.
  - Codex skills may come through an overlay `CODEX_HOME` that links back to the real `~/.codex` (including `auth.json`), if a test shows that Codex loads the skills and its sessions still land in the real folder.
  - Build the hook status feed, Codex app-server desks and ACP desks.

Evidence: `docs/research/harness-hub/`.

## OB-011 — How should assistant agents reach the building (MCP transport and auth)? — DECIDED 2026-10-01: Defer for now, later a proper server deployment of the app itself might happen…

Decided in Office Building on 2026-10-01: Defer for now, later a proper server deployment of the app itself might happen and then we’ll come back to this.

The MCP server (OB-010 in TODO.md) lets assistant agents list
floors and desks, read the decisions queue, answer calls and hire agents. It
exposes write actions on your Mac, so how it listens and who may call it is
your call.

**Options.**

- **Streamable HTTP on 127.0.0.1 with a bearer token (recommended)** — the
  same shape as other local HTTP MCP servers (`claude mcp add --transport http … --header
  "Authorization: Bearer …"`). The app writes a random token to a mode-600
  file on first run. Works for any local client, and a server-side agent can
  reach it through the SSH tunnel you already use. Cost: one more local port.
- **stdio through a small launcher** — the client starts
  `office-building-mcp`, which forwards to the running app over a Unix
  socket. No port and no token, but only clients on this Mac can use it, and
  each client needs the launcher path.
- **Defer** — keep the building GUI-only until an assistant actually needs it.

**Recommendation.** Streamable HTTP with a token file. It matches the HTTP
MCP servers assistants already use, so their config pattern is the same.
