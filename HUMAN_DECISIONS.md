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

## OB-059 — How should the building tell me an agent has stopped, and where do the app's settings live? — DECIDED 2026-11-12: one app-level Settings panel, with the alert details left to the build

Decided by A Human on 2026-11-12, before an unattended build of GitHub issue #1:

- Settings is one app-level panel. It opens with Cmd+, from the building view and from any floor, and from a new object on the Setup floor, a fuse box beside the toolbox. Cmd+, keeps it reachable if the Setup floor is demolished.
- Commits stay local. Nothing is pushed until A Human approves.

Calls the build made while A Human was away. Issue #1 gave no design, so everything about the alerts themselves is in this list. Each stands until A Human says otherwise:

- The building alerts on three events, each with its own switch and all on by default: an agent finished its turn, an agent needs you (a question or a permission prompt), an agent is stuck on an error.
- There are three ways to deliver an alert, each with its own switch and all on by default: a soft system sound, a Dock bounce, a system notification. The system notification shows only when the app window is not in front.
- "Frequency" is one cooldown, at most one alert every N seconds (default 10). Alerts inside the window are merged into one.
- No alert for anything on the first scan after launch.
- "No alert for the desk whose panel is open" is built as no alert for that desk while the app window is in front. With another app in front, an open panel does not mean anyone is watching it.
- Clicking a notification only brings the app forward in this version.
- Left for later, and filed in TODO.md (OB-060 to OB-063): an alert when an agent leaves the floor, per-floor mute, an "only agents hired in the building" switch, and opening the right desk on a click.
- Settings are kept by the Rust side in `settings.json` in the app data folder, so that background work can read them later. The webview's own preferences stay where they were.
- Settings is a dialog plate, not a sidebar section, because it has to open from the building view, which has no sidebar. It never shares the screen with another plate or the docked terminal.
- A change in Settings saves at once. There is no Save button.
- The Dock bounce is also skipped while the app is in front, since macOS ignores it then.
- The "flash" in the issue is the Dock bounce. An in-window flash is TODO OB-065.

This amends no earlier ruling. Built as TODO OB-057, with the checks for A Human in HUMAN_TODO OB-058.

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
