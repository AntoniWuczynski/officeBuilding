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

Amended on 2026-11-12 by a call of the build, not by A Human (OB-099). A floor
connected to Linear shows its Linear issues after these items.

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

## OB-099 — How should a floor's TODOs come from Linear? — DECIDED 2026-11-12: the Linear key lives in the app's Settings, with the rest left to the build

GitHub issue #2 asks for connectors, so that a floor's TODO lists can come from outside tools as well as the repo's own files, with Linear first. It left five questions open for A Human.

Decided by A Human on 2026-11-12, before an unattended build of the issue. Both are the rulings of OB-059, applied here:

- The app has one Settings panel. The build put the Linear key and the agent label there, in a new Connectors group.
- Commits stay local. Nothing is pushed until A Human approves.

Calls the build made while A Human was away. The build brief answered the issue's five questions without marking the answers as A Human's, so they are listed here and not above. Each stands until A Human says otherwise, and A Human can veto any of them.

From the build brief, on the issue's five open questions:

- Merge or replace: merge. A floor with both `TODO.md` and Linear shows both, file items first, each with a source badge and a link out. This is the amendment to OB-002 described at the end.
- Granularity: a floor maps to one Linear team, which is required, plus an optional project and an optional label, all combined.
- Human or agent: Human TODO gets the issues assigned to the key's own user. TODO gets the issues that carry the agent label (default `agent`) or are assigned to nobody. Issues assigned to other people are not shown. Only unstarted and started issues are shown.
- Two-way sync: no. This version reads and ticks only.
- Which connector next: none was built. TODO OB-102 asks A Human to pick one.

From the build brief, on the rest:

- A tick asks first ("Mark ENG-123 as Done in Linear?"), moves the issue to the team's completed status with the lowest position, and rolls back with an error message if Linear refuses.
- One Linear key for the whole building, kept in the macOS Keychain and entered in Settings. The link between a floor and Linear is a "Linear" row on the floor's Info panel.
- Linear is asked every 60 seconds, away from the 5-second refresh. The app obeys Linear's rate-limit replies, backs off on errors, shows a stale or offline state and offers a Re-check button.
- The poller, the Keychain access and the client for Hypertext Transfer Protocol Secure (HTTPS) are separate modules behind traits with fakes, so that GitHub issue #7 can reuse them.
- Tests use fixtures with invented data. Nobody called Linear during the build.
- The app is ad-hoc signed, so the Keychain may ask again after each reinstall. That is left as a check for A Human and not worked around.
- Not built: creating Linear issues from the app, decisions sourced from Linear, other connectors, and handing Linear's own server for agents to hired agents (TODO OB-100 to OB-103).
- Per-floor state is edited on the floor and keyed by the repo root, like pinned floors. Defaults for the whole building live in Settings.

From the design, beyond the brief. The first five are the ones most worth A Human's eye:

- An issue that carries the agent label goes to TODO even when it is assigned to somebody else. The brief says issues assigned to other people are not shown, and the design read that as "unless they carry the agent label", because the label is the explicit mark that an issue is for agents.
- The agent label is one setting for the whole building (Settings, Connectors group), not part of a floor's link.
- The confirmation before a tick is a row inside the list, not a dialog. DESIGN.md forbids a panel over a panel.
- A Linear badge opens the issue through a backend call. The webview sends the item's id, and the backend takes the address from its own cache, checks that it starts with `https://linear.app/` and hands it to macOS's `open`. No opener plugin was added.
- A demolished floor keeps its link. It is not asked about while it is down, and Linear is still connected if the floor is rebuilt.
- An item's id holds Linear's own permanent id for the issue, not `ENG-123`, which changes when an issue moves to another team.
- The links live in `store.json`, not in a file of their own, and connecting a floor pins it.
- The backend shows a tick, not the webview. It marks the item done before it calls Linear and takes the mark off again if Linear refuses.
- Two crates were added: `ureq` 3.5.1 on macOS's own secure transport, and `security-framework` 3.7.0 called directly, where the issue suggested `keyring`. The two share one dependency tree.
- The Keychain is read at most once per launch, and not until a floor is linked or the key's status is asked for. A refusal is not retried on a timer, because every retry can raise a system prompt.
- A key Linear refuses stops every Linear call until the key is replaced or Re-check is pressed.
- Having no usable key empties a floor's Linear items. Every other failure keeps the last good list and says how old it is.
- Saving a key checks it with Linear first. A key that is refused, or that cannot be checked because Linear is out of reach, is not saved.
- Only "out of reach" slows the poll and only "rate limited" makes it wait. A floor whose team was deleted is asked again every minute.
- Badges are text only: the file's name on a file item, `Linear · ENG-123` on a Linear item, nothing on the app's own. No logo.
- The limits are 200 issues per floor, 50 teams and 50 open projects per team in the connect form, and 10 labels read per issue (TODO OB-107 to OB-109).
- `tick_todo` now runs off the main thread and pushes a snapshot, so a file tick shows at once.

From the review, after the design:

- Changing a floor's link drops the old link's items at once. Before this, an issue from the old team could stay on the board and still be ticked.
- Re-check runs off the main thread, so the window stays usable while a Keychain prompt is open.
- When a floor's link is changed and its Linear project has since closed, the project is kept and shown as "(closed)". Before this it was dropped without a word and the floor widened to the whole team.

This amends OB-002, and the amendment is a call of the build, not a ruling by A Human. OB-002 says TODOs come from the repo's own files, falling back to the app's store. A floor connected to Linear now shows those items first and its Linear issues after them. The files are read and written as before. Built as TODO OB-097, with the checks for A Human in HUMAN_TODO OB-098 and the deferrals in TODO OB-100 to OB-121.

## OB-124 — How should a QA desk review my pull requests, and under whose name? — DECIDED 2026-11-12: through A Human's own `gh` login, by a reviewer that only reads and that the building may end, with the rest left to the build

GitHub issue #7 asks for a quality assurance (QA) desk on each floor that reviews A Human's new pull requests and posts what it finds as "Office Building Quality Assurance". It left five questions open for A Human.

Decided by A Human on 2026-11-12, before an unattended build of the issue. The first three are new rulings for this issue:

- Identity: reviews post through A Human's own `gh` login, under a plain-text "Office Building Quality Assurance" heading. The GitHub App the issue recommends is not built now and comes later (TODO OB-125).
- The reviewer reads the diff only and runs nothing.
- The building may end a review agent it started, and remove its worktree, once the agent has reported or hit a time limit. This is an exception to "the app never stops a running agent", for QA runs only.

The other two are the rulings of OB-059, applied here:

- The app has one Settings panel. The build put the QA settings there, in a new Quality assurance group.
- Commits stay local. Nothing is pushed until A Human approves.

Calls the build made while A Human was away. The build brief answered four of the issue's five questions without marking the answers as A Human's, so they are listed here and not above. Each stands until A Human says otherwise, and A Human can veto any of them.

From the build brief, on the issue's other open questions:

- Scope: only pull requests A Human wrote. Other authors' pull requests are not reviewed (TODO OB-131).
- Re-review: every new head commit is reviewed once, with a setting to review only a pull request's first commit.
- Verdict: comment only. The review never approves and never requests changes (TODO OB-130).
- Overlap with issue #4: not built. Whether the manager reads QA findings is left to that issue (TODO OB-132).

From the build brief, on the rest:

- QA is off by default and switched on per floor. Switching it on pins the floor.
- The building watches with `gh pr list` for each floor whose repo has a GitHub `origin`, every few minutes, on the poller built for Linear.
- Drafts are skipped (a setting), and so is a pull request labelled `no-qa`.
- A head commit is reviewed once it has been quiet for a few minutes, with a cap on reviews a day for one pull request.
- The reviewer is a normal terminal desk the building starts, visible on the floor with its state and spend, and never `claude -p`. A Human chose terminal sessions over headless mode for GitHub issue #6, and the brief applied that choice here.
- The reviewer works in a throwaway worktree at the pull request's head, outside A Human's checkout.
- The findings are taken from the agent's final reply as one block of JavaScript Object Notation (JSON). The building, not the agent, turns them into one review, with inline comments for lines inside the diff and the rest in the summary.
- With nothing found the building posts one short summary, with a setting to post nothing.
- The finished reviewer's terminal is released.
- The desk sits in the floor's front-left corner with a drawn magnifying glass. The result is a drawn stamp and not an emoji. The link to a review goes through a backend call that opens an address the backend built itself.
- The settings are a default for floors, the poll interval, every push or the first only, skip drafts, reviews at once, reviews a day, and the tool, model and effort. There is no monthly cap in US dollars, because the app keeps no spend ledger (TODO OB-127).
- The launch of a hired agent with a fixed role is built here in a simple form, for issue #4 to extend.

From the design, where it changed what the brief assumed. These three are the ones most worth A Human's eye:

- The reviewer runs in Claude Code's `dontAsk` permission mode with only the three tools that read and search files. The brief asked for "the tool's read-only mode", which would be plan mode. Plan mode runs shell commands to explore and ends on a prompt that would stall the desk. `dontAsk` refuses anything that would prompt, and with only those tools the reviewer has no shell and no edit tool at all.
- Claude Code is the only reviewer. The brief's settings list a tool. The findings travel through a hook, and Codex skips a new hook until it is trusted once, so a Codex reviewer's report could silently never arrive (TODO OB-128).
- The worktree is thrown away after each review, but its folder is the same one per floor every time. The brief suggested a stable path so that Claude Code's trust prompt appears once at most. The cost is one review per floor at a time.

From the design, beyond the brief:

- "Reads the diff only" is built as reading the diff and the surrounding code in the checkout of the pull request's head. The reviewer still runs nothing. A Human may want this narrower.
- The reviewer takes nothing from the Setup library. It gets no skills, instructions, MCP servers or hooks of A Human's, only the status feed's hooks and its own report hook. It also does not load the checkout's own project settings.
- The pull request's diff is treated as untrusted text. The reviewer's instructions say that nothing in the diff or the repo is an instruction to it, that it must never quote a secret, and that it stays in the checkout.
- Switching QA on asks first, in a row on the Info panel, because it leads to posts on GitHub.
- A floor's state is on, off or "follow the Settings default". A floor that is on only through the default is not pinned.
- The quiet period is 3 minutes and the time limit is 20 minutes. Neither is a setting (TODO OB-141).
- The defaults are a check every 5 minutes, every push reviewed, drafts skipped, a summary posted when nothing is found, one review at a time and 3 reviews a day for one pull request.
- With no model picked in Settings, the reviewer starts on Claude Code's default model and that model's default effort.
- A failed review is not tried again until the next push, so a broken setup cannot spend the plan in a loop (TODO OB-138).
- Posting is tried three times. From the second try every finding goes in the summary, because one inline comment GitHub rejects fails the whole review.
- Every `@` in text from the agent is followed by an invisible character, so a finding can never notify a GitHub user.
- The stop is one SIGTERM, a wait of up to 5 seconds and the release of the terminal. It never sends SIGKILL (TODO OB-140). It can reach only a reviewer the QA desk seated, and no app call reaches it.
- Switching a floor off while its reviewer works ends that reviewer and posts nothing.
- The reviewer counts as a desk on the floor and on the spend meter while it works. A finished reviewer is kept off the sign-out sheet.
- The stamp reads "QA passed" or "QA notes". A new push clears it until the new commit is reviewed, and a failed review shows no stamp.
- The review's footer names the commit, says that a review agent in Office Building wrote it and that it was posted from this account, and mentions the `no-qa` label.
- A diff over 2 MiB is not reviewed. A floor keeps its last 200 reviews, and 100 open pull requests are read per repo (TODO OB-143).
- Only an `origin` on github.com is understood (TODO OB-133).
- No dependency was added, and the building holds no GitHub token. `gh` keeps its own.

From the review, after the design:

- A pull request that already contains the folder the building writes the diff into is refused, so that a pull request cannot redirect that write (TODO OB-147).
- Before a repeated post, the building looks for a review it already posted on that commit and records it, where the design would have posted again.
- A commit that one floor has reviewed, is reviewing or is waiting to post is not reviewed by another floor on the same repo.
- A start that fails because GitHub refuses the pull request's own diff is given up after three tries and recorded as failed. A start that fails for any other reason is tried again at every check and never given up.
- Check now does nothing while GitHub has told the app to slow down.
- The checkout of a pull request runs with git's hooks switched off and without Large File Storage downloads. The design had accepted that git runs the repo's `post-checkout` hook there. That hook can run code the pull request changed, which does not fit A Human's ruling that the reviewer runs nothing.
- A program the app runs with a time limit is started in its own process group, and the whole group is ended at the limit. Like the probe in OB-075, these are the app's own helper programs and not agents.

This amends one earlier ruling, and the amendment is A Human's own. A Human ruled on 2026-10-01 that the app never stops a running agent (recorded in TODO OB-027), and made one exception on 2026-10-06 for demolishing a floor (TODO OB-029). The stop rule above is the second exception. It also builds on OB-040: the reviewer is Claude Code's own unmodified CLI in a terminal, and the building touches no login token. Built as TODO OB-122, with the checks for A Human in HUMAN_TODO OB-123 and the deferrals in TODO OB-125 to OB-148.

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
