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

## OB-075 — How should a desk's code open in an editor? — DECIDED 2026-11-12: the default editor is picked in the app's Settings, with the rest left to the build

A Human asked for this on 2026-11-11 (GitHub issue #5): a button that opens the code an agent is working on, per session, so that an agent in a linked git worktree opens its own checkout and not the floor's main one.

Decided by A Human on 2026-11-12, before an unattended build of the issue. Both are the rulings of OB-059, applied here:

- The app has one Settings panel, so the default editor is picked there, in a new Editor group.
- Commits stay local. Nothing is pushed until A Human approves.

Calls the build made while A Human was away. The issue left four questions open for A Human, and the build brief answered them without marking the answers as A Human's, so they are listed here and not above. Each stands until A Human says otherwise.

From the build brief, on the issue's four open questions:

- Default editor: the first one found, in the order Cursor, VS Code, Zed, Devin Desktop, until another is picked in Settings.
- What opens: the top level of the checkout the agent is in now, not its exact folder. For an agent in a worktree that is the worktree's own folder.
- Window: no window flag is passed, so each editor keeps its own default. TODO OB-076 asks A Human whether that is enough.
- Floor button: yes, one button in the Info panel that opens the floor's main checkout.

From the build brief, on the rest:

- The editors are Cursor, VS Code, Zed and Devin Desktop, which is the new name of Windsurf. The old `windsurf` name is still looked for.
- The editor's own command is preferred, by the absolute path the login shell reports. Failing that, the app is opened with `open -a`.
- The button sits in the header of desks that have a panel, on sign-out sheet rows and in the Info panel. Hand-started desks have no panel, so every desk's worker also gets a right-click menu. A helper's tiny desk gets the same menu when the helper is in a worktree and the pod is big enough to seat people.
- The shortcut is Cmd+Shift+O while a desk's panel is open.
- The worktree's branch is shown in the tooltip, read from the worktree's HEAD file.
- The folder is found by walking up to `.git`, as discovery already does. The app runs no `git`, where the issue proposed `git rev-parse --show-toplevel`.
- The webview sends an id and an editor from the fixed list, never a path. The folder is checked to exist at the click.

From the design, beyond the brief:

- The app call takes a place (a desk, a helper or a floor) where the issue proposed a session id. Live and signed-out sessions share one kind, since their ids never collide.
- The folder to open is kept in its own map beside the folder a session resumes in. The two answer different questions.
- A session's folder follows its latest folder only while that is in the repo it started in. Otherwise it is the checkout it started in. A helper's folder is kept only when it is in its session's repo. This limits what a transcript, which an agent writes, can make the button open.
- The list of installed editors is remembered between clicks, refreshed whenever the list is asked for, and forgotten when a launch fails. A click does not wait for a login shell.
- The backend picks the default editor and names the editors. Before the list has loaded, the interface says "Open in editor".
- A command that sits inside another listed editor's app bundle is not counted. Cursor ships a `code` command of its own, and without this rule "Open in VS Code" could open Cursor.
- A launch waits at most 10 seconds for the command to exit. Exit code 0, or still running after 10 seconds, counts as opened. Anything else tries the next route and then reports the error.
- The outcome is a toast. There is no error text on the row itself.
- The "Open in…" list is a menu, not a panel. It is a small paper plate drawn outside the scene, so the rule that a panel never overlaps a panel does not cover it (recorded in DESIGN.md).
- A worktree helper's figure becomes a button. A click and a right-click both open the menu, since a helper has no other click action, and that makes it reachable by keyboard.
- Cmd+Shift+O also acts on the worker that has keyboard focus when no panel is open. The brief gave it only for an open panel.
- The branch is shown only for a session or helper in a worktree whose HEAD names a branch. A detached HEAD shows none.
- No dependency was added. Tests that need a real repository run the test machine's `git`.
- Discovery remembers a session's last checkout in one case only: the folder has been deleted and the session is still inside it. Without that, "that worktree is gone" would last a few seconds and the button would then open the main checkout without saying so.
- A panel drawn without the editor list shows no button, where the terminal's own parts treat a missing provider as an error.

From the review, after the design:

- Programs the app starts for this (the detection probe and the editor) do not inherit the launcher's session and terminal identity. Without this an agent's messaging token reached the editor's terminals.
- The detection probe gets 5 seconds, after which the app ends it and falls back to looking for the apps. The probe is the app's own login shell, not an agent, so the ruling in TODO OB-027 that the app never stops a running agent is untouched.

This amends no earlier ruling. It builds on OB-032 and OB-055 in TODO.md (an agent in a worktree stays on its repo's floor, and a `cd` does not move it). Built as TODO OB-073, with the checks for A Human in HUMAN_TODO OB-074 and the deferrals in TODO OB-076 to OB-096.

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
