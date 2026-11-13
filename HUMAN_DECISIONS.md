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

Followed up on 2026-11-12 (OB-181). A Human ruled that a Claude Code desk gets a chat view as an ordinary Claude Code session in the app's own terminal, drawn from its transcript and the hook feed, with no `claude -p`. That widens the hybrid line above, since Claude Code has no structured protocol here. Everything else about the chat view is a call of the build, listed in OB-181.

Amended on 2026-11-12 by A Human (OB-210). The line "never touches login tokens" now has one exception. A session's own login header may pass through the building's loopback router on its way to the same vendor. It is never stored, never logged and never sent to another vendor. Whether Anthropic's terms allow this for a Claude login is an open call (OB-211).

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

## OB-152 — What may a floor's manager do, and how does it reach the building? — DECIDED 2026-11-12: it only orchestrates, with no shell and no file editing, and its tools reach the app over a Unix socket kept to managers, with the rest left to the build

GitHub issue #4 asks for a project manager on each floor. It is an agent in the manager's office that sees what each desk is doing, sends the desks instructions, hires help and escalates real decisions to A Human. A Human asked for it on 2026-11-11.

Decided by A Human on 2026-11-12, before an unattended build of the issue. The first two are new rulings for this issue:

- The manager's tools may reach the running app through a small helper program that talks to the app over a Unix socket. This is a narrow follow-up to OB-011, which deferred any server for assistant agents. As the build brief records it, the socket is owner-only in the app's data folder, each manager gets a random token for its launch, passed in its environment and never on a command line, each connection is tied to one floor, and nothing listens on a network port. The wider server for assistant agents stays deferred.
- The manager only orchestrates. It has read-only file tools and the building's tools, with no shell and no file editing.

The other two are the rulings of OB-059, applied here:

- The app has one Settings panel. The build put the manager's settings there, in a new Manager group.
- Commits stay local. Nothing is pushed until A Human approves.

What A Human wrote in the issue on 2026-11-11, and where this build departs from it. Each departure is a call of the build, listed again below, and A Human can veto any of them:

- One manager per floor. Built as written.
- The tool is picked in Settings. Built with one choice, Claude Code (TODO OB-154).
- Act by default, and it may hire without asking. Built with hiring switched off on every floor until A Human switches it on there, and with no Propose mode (TODO OB-153).
- Hand-started desks are read and messaged. Not built. A note to a hand-started desk waits and is shown, and the manager cannot read such a desk (TODO OB-155, OB-156 and OB-159).
- It may drive T3 Code threads. Not built. The build brief records that A Human skipped GitHub issue #3 on 2026-11-12, and every T3 route with it (TODO OB-160).
- Every thread the manager drives stays open to A Human, and A Human's message wins over a waiting note. Built as written.

Calls the build made while A Human was away. The build brief set out most of the design without marking it as A Human's, so it is listed here and not above. Each stands until A Human says otherwise, and A Human can veto any of them.

From the build brief:

- One manager per floor, hired from the manager's office, with its tool, model and effort taken from Settings. The brief named Claude Code or Codex.
- The manager's tools are `list_desks`, `read_desk`, `send_to_desk`, `wait_for_desk` with a capped wait, `hire_desk`, `list_todos`, `tick_todo`, `add_todo`, `log_decision` and `escalate`. A manager cannot touch another floor.
- A structured desk gets a note as a prompt, queued while a turn runs. A terminal desk the app started gets the text typed in only when it is idle and never while it waits on a prompt. Otherwise the note is queued. No keystrokes go into a window the app does not own.
- Hand-started Claude Code desks were to be reached through a Claude Code manager's own `SendMessage` tool, once that route was checked against Anthropic's pages. Every other hand-started desk shows the note as queued.
- Every delivered note is stored as a message between agents, so it is walked across the floor.
- The manager never stops an agent and never answers a permission prompt.
- The caps are 3 desks hired by the manager at once and 20 US dollars on the summed spend of desks it hired, both adjustable. A desk with no price data counts toward the desk cap only, and the panel says so.
- Two modes, Act by default and Propose. A Human can pause the manager on a floor in one click, and managed desks carry a "managed" mark.
- The boss cut-out becomes the manager. With nobody hired the chair is empty and the desk carries a "Hire a manager" sign. A Manager panel shows its plan, its queue and the log of what it told whom.
- `escalate` adds an open call to `HUMAN_DECISIONS.md`, or to the app's store when the repo has no such file, as OB-002 rules.
- Manager hires are off until A Human switches them on for a floor. Settings for the whole building live in Settings, and a floor's own state is changed on that floor.

From the design, where it changed what the brief or the issue assumed. These eight are the ones most worth A Human's eye:

- Hiring by the manager is off on every floor. The brief says both "Act (default, may hire)" and that manager hires are off until A Human switches them on. The design took the careful reading. A floor has no manager until A Human hires one, and that manager cannot hire desks until "May hire desks" is switched on for the floor.
- `SendMessage` is not used, although Anthropic documents it. A send of that kind leaves the building. It can reach any Claude Code session on the Mac, and the floor binding, the pause, the length cap and the stripping of control characters would all rest on a hook that maps a session's name back to a desk, which is not documented. The manager is launched without `SendMessage` and `ListAgents`. So the manager cannot message a hand-started Claude Code session, which the issue's acceptance list asks for (TODO OB-155).
- Claude Code is the only manager. Codex reads files only through its shell, so A Human's ruling of no shell cannot be met with it (TODO OB-154).
- Propose mode is not built, and neither are `tick_todo`, `add_todo` and `log_decision`. The issue did not fit the build, and these were cut from the end of the brief's order of priority (TODO OB-153 and OB-158). `escalate` and `wait_for_desk` were kept.
- The Manager panel cannot send the manager home. The building never stops a running agent, and A Human's exception in OB-124 covers QA reviewers only. A Human ends the manager by typing `/exit` in its terminal (TODO OB-174).
- Both fixed roles refuse messages from other Claude Code sessions. This changes the QA reviewer of OB-124 too, because any Claude Code session on the Mac could otherwise write to it.
- The manager is a session like the QA reviewer. In the floor view it sits in the office. In the tally, the floor light, the building view and the alerts it counts as an ordinary session (TODO OB-170).
- "Never stops an agent" and "never answers a permission prompt" are kept by taking tools away. They do not rest on the manager's instructions. It has no shell, and no path from one of its tools reaches a stop, an approval or a keystroke other than a gated note.

From the design, beyond the brief:

- The helper is the app's own program started in a helper mode, which never opens the window. It speaks the Model Context Protocol (MCP) to Claude Code and passes each message to the socket.
- The token is 32 random bytes from macOS, kept in memory only and compared in constant time. A wrong token is refused after a short pause with no reason given. It is revoked when the manager's terminal exits.
- The app serves at most 8 connections on the socket, and caps the length of every line.
- `wait_for_desk` waits 50 seconds at most, so that a later Codex manager would fit Codex's 60-second limit.
- The other limits are 4,000 characters a note, 2,000 a task, 5 waiting notes a desk and 20 a floor, 200 lines and 8,000 characters a read, and 6 options an escalation. Text over a limit is refused and not cut short.
- A note is typed only when the hook feed says the desk is idle and the session's own files agree. So notes to opencode, Cursor and Antigravity desks in the app's terminals wait, because those tools do not report when they are idle (TODO OB-157).
- Nothing is typed into a terminal within 60 seconds of A Human's last keystroke there.
- A typed note always starts with a fixed introduction, "Note from this floor's manager", so its first character can never be one a tool reads as a command.
- Text from the manager is stripped of escape sequences, control characters and invisible characters before it is typed, stored or shown. The interface shows it as plain text only.
- One note goes to a desk at a time, oldest first, and the next waits until the desk has picked the last one up. Notes never expire. A Human can withdraw one in the panel. A note is lost when its desk or the manager leaves.
- A note that A Human's message replaced is not sent again.
- A paused manager can still read the floor. Its sends, hires and escalations are refused, and its waiting notes are held until A Human resumes it.
- The spend that counts toward the ceiling leaves out the manager's own, and is kept in memory only (TODO OB-162). At the ceiling no desk is hired and no new note goes to a desk the manager hired. The building does not stop those desks.
- The manager hires only Claude Code and Codex desks in the app's terminals, in the floor's own folder (TODO OB-163). A hired desk gets no token and no special permissions.
- `read_desk` reads only desks the app started (TODO OB-159).
- An escalation is a new `MGR` section at the end of the floor's human decisions file. The building opens that file for appending only, refuses a symbolic link and never creates the file. The title and options are cleaned so that the manager cannot write a heading, a status or an option of its own making.
- Hiring a manager pins the floor. Its first task makes it call two of its tools, so the panel can say that its tools answer.
- The Manager panel's Plan is the floor's open agent TODOs. The manager writes no plan of its own (TODO OB-161).
- The manager's desk light is green while it works, yellow while a call it escalated is open and off while it is paused.
- Model and effort apply to the next manager hired. The caps apply at once.
- A source test fails the build when one of the manager's files names a call that stops an agent, answers for A Human or writes to a terminal (TODO OB-177).
- No dependency was added.

From the review, after the design:

- Any key A Human presses in a terminal counts as typing, apart from the reports a terminal sends by itself. A line A Human has typed and not sent holds that desk however old it is, until Enter or Ctrl-C (TODO OB-167).
- Managers are hired one at a time, so two hires at once cannot seat two managers on a floor or swap its token.
- A manager whose record could not be saved keeps its seat, and the record is caught up at the next save. The first fix ended the manager's process, which the rule against stopping an agent forbids.
- The manager may have at most 10 open escalations on a floor.
- The app does not remove a socket that a running copy of the app still answers (TODO OB-173).
- A manager that left while it was being seated is not recorded as hired.

This amends one earlier ruling, and the amendment is A Human's own. OB-011 deferred every way for an agent to reach the building. A Human now allows one, the socket above, for managers the building itself starts. OB-011 otherwise stands, and the server for assistant agents (TODO OB-010) is still not built. A Human's ruling of 2026-10-01 that the app never stops a running agent (TODO OB-027) is not amended, and the manager gets no exception. The build changed one thing in OB-124 by its own call, which is that the QA reviewer now refuses messages from other sessions. It also builds on OB-002, since an escalation goes into the repo's own decisions file or the app's store, and on OB-040, since the manager is Claude Code's own unmodified CLI in a terminal and the building touches no login token. Built as TODO OB-150, with the checks for A Human in HUMAN_TODO OB-151 and the deferrals in TODO OB-153 to OB-178.

## OB-181 — How should a desk get a chat view, and how does Claude Code get one? — DECIDED 2026-11-12: a Claude Code chat desk is an ordinary Claude Code session in the app's own terminal, drawn from its transcript and typed into when idle, with no `claude -p`, and the rest left to the build

GitHub issue #6 asks for a setting that switches a desk from the terminal to a chat of the building's own, the same for every tool. A Human asked for it on 2026-11-11. It is a follow-up to OB-040.

Decided by A Human on 2026-11-12, before an unattended build of the issue. The first is a new ruling for this issue:

- A Claude Code chat desk is a normal interactive Claude Code session in the app's own terminal. The chat is drawn from the session's transcript file and the hook feed. What is typed in the chat's message box is typed into the terminal once Claude Code is idle. There is no `claude -p` and no Agent SDK. As the build brief records the same ruling, a permission prompt shows as a card with one click that flips to the terminal, where it is answered, and replies appear message by message.

The other two are the rulings of OB-059, applied here:

- The app has one Settings panel. The build put the choice of view there, in a new Agents group.
- Commits stay local. Nothing is pushed until A Human approves.

The issue left four calls open for A Human: the default view, whether a desk may switch views, how Claude Code is driven, and what the first version must hold. The build brief answers all four and marks only the Claude Code answer as A Human's. The other three are therefore listed below as the brief's, not above.

What A Human wrote in the issue on 2026-11-11, and where this build departs from it. Each departure other than the first is a call of the build, listed again below, and A Human can veto any of them:

- Claude Code through a new driver over `claude -p` in its streaming mode. Replaced by A Human's ruling above.
- A setting, Terminal or Chat, that new desks follow and the hire dialog can override. Built as written. The optional default for each tool is not built.
- One chat for every tool, with differences as capability flags only. Built as written, with five flags and not the four the issue names (TODO OB-201).
- Antigravity stays in the terminal and shows why. Built as written. The build brief asked for more, and that was cut (TODO OB-184).
- A hand-started session can be read in the chat and continued as a chat desk. Not built (TODO OB-182, OB-193 and OB-194).
- A desk switches view at idle by ending and resuming its session. Not built. A Claude Code desk flips at any time because its terminal is always there, and every other desk keeps the view it was hired with (TODO OB-192).
- Plans and to-dos, nested sub-agent threads, a model and effort switcher and attachments. Not built (TODO OB-183 and OB-189 to OB-191).
- Streaming on every desk. Claude Code's replies appear message by message, which follows from A Human's ruling (TODO OB-198).

Calls the build made while A Human was away. The build brief set out most of the design without marking it as A Human's, so it is listed here and not above. Each stands until A Human says otherwise, and A Human can veto any of them.

From the build brief:

- The setting is in Settings, in an Agents group, as View with Terminal or Chat. Terminal is the default. The hire dialog's switch starts at the setting and can change it for one hire. The view is fixed at hire.
- One chat for every tool, split into components, with differences expressed only as capability flags.
- The first version holds Markdown with code blocks and raw HTML off, tool cards that fold with the command, the output and file changes drawn as diffs, inline approvals, plans and to-dos where the tool shares them, folded thinking, a message box with waiting messages, and Stop.
- A Claude Code desk can flip between chat and terminal at any time, because its terminal is always there.
- Hand-started Claude Code sessions were to get the same view, read-only.
- Antigravity was to be included through its `agy acp` server on the existing Agent Client Protocol (ACP) driver, with the greyed-out reason kept for tools that cannot do Chat.
- The chat stays in the docked panel, which opens taller for a chat, at about 60% of the window and still resizable, in the existing dark palette.
- Deferred: the model and effort switcher, images and attachments, nested sub-agent threads, live switching for tools other than Claude Code, a read-only chat for hand-started Codex sessions, and "Continue here".

From the design, where it changed what the brief or the issue assumed. These nine are the ones most worth A Human's eye:

- Antigravity's chat is not built, although the brief said to include it. The server exists, but nobody could run it in an unattended build to see whether its model and thinking-level pickers work with the existing driver. Chat is greyed out for Antigravity with the reason. It needs a live probe with A Human present (TODO OB-184).
- The read-only chat for hand-started Claude Code sessions is not built, although the brief said to include it. It was cut for size. The backend can already serve it, and what is missing is a way to open it (TODO OB-182).
- Plans and to-dos have no cards of their own, although the brief's first version lists them. Cut for size (TODO OB-183).
- Every tool now queues a message sent while a turn runs. Before this change Codex took such a message into the running turn, so steering a Codex turn from the panel is gone (TODO OB-188).
- A message A Human sends from the chat replaces the manager's waiting notes for that desk and is typed ahead of any later one. This follows OB-152, where A Human's message wins over a waiting note.
- Stop is refused while a permission prompt is open. In a Claude Code terminal Stop is one Escape key press, and Escape on a prompt declines the action, which would be answering for A Human.
- After A Human's Stop a Claude Code desk counts as idle for A Human's own messages once Claude Code's own files say so, because Claude Code's Stop hook does not fire on an interrupt. Manager notes keep the stricter rule.
- Links cannot be opened from a chat, and images are never loaded. Both show as text with their addresses (TODO OB-185 and OB-190).
- Two dependencies were added, pinned exactly: `react-markdown` 10.1.0 and `remark-gfm` 4.0.1.

From the design, beyond the brief:

- The app calls keep their `structured_*` names and now serve Claude Code chat desks too (TODO OB-200).
- A Human's messages have a queue of their own beside the manager's, served first and kept in memory only (TODO OB-195). The manager's files cannot name it.
- A message is cleaned as a manager's note is, and is capped at 16,000 characters. It is never typed while a prompt is open, while the terminal holds an unsent line or while the desk is busy. A Human's own recent typing does not hold A Human's own message.
- A message from the chat is not stored as a message between agents and is not walked across the floor. The transcript shows it.
- Stop interrupts a turn and never ends a session. On a Claude Code terminal it is sent at most once in 2 seconds, and not within 2 seconds of A Human typing there, because two Escapes close together clear a draft or open Claude Code's rewind menu.
- The transcript's path is never taken from the webview or a hook. It is found from the session id, must be a regular file under Claude Code's own projects folder and is opened without following a link.
- The caps are the last 8 mebibytes of a transcript and 1,000 entries in the reader, 2 mebibytes a line, 300 entries and 600,000 characters a backlog, 400 entries in the panel, and 40,000 characters of text, 20,000 of output and 20 diffs of 20,000 characters an entry (TODO OB-197).
- Sub-agent records in a transcript are skipped (TODO OB-191).
- A diff is the removed lines followed by the added lines, with no syntax colouring (TODO OB-199).
- Only an agent's message is drawn as Markdown, from a fixed list of elements. A user's message, a thought, a notice, a tool's title, its command, its output and its diffs are plain text.
- A desk the manager hires opens in the terminal view (TODO OB-202). A Claude Code session resumed from the sign-out sheet follows the setting.
- Which face a panel shows is the panel's own state, remembered per session while the app runs and not across a restart (TODO OB-196). A chat remembers its own height, apart from the terminal's.
- The first message to a Claude Code chat desk hired with no task waits until Claude Code reports that it is idle, which can take about a minute (TODO OB-187).
- A hire that asks for Chat from a tool with no chat view is refused with the reason and starts nothing.

From the review, after the design:

- The message box ignores a second Enter or click while a send is under way, and a refused send keeps the text.
- A desk A Human stopped counts as idle when the building decides whether its last message was picked up, so one Stop lets one waiting message through and not two (TODO OB-205).
- Opening another desk no longer makes the app's main thread wait, and each switch carries a number so that an older one cannot undo a newer one (TODO OB-206).
- A tool that waits on an approval reads "Waiting" on its card.
- The Chat and Terminal switch takes the look of the "Open in" button, with no new colour.

This is a follow-up to OB-040 and amends one line of it, and that amendment is A Human's own. OB-040 ruled "terminal by default, plus a structured protocol where the harness has an official one". Claude Code now has a chat with no structured protocol, drawn from its terminal session. The rest of OB-040 stands. The building runs each tool's own unmodified CLI, never an agent loop of its own, and touches no login token. The issue raised two risks for a headless Claude Code driver, Anthropic's policy on third-party products and the usage limits. No such driver is built. A Human's ruling of 2026-10-01 that the app never stops a running agent (TODO OB-027) is not amended. Stop interrupts a turn at A Human's click and ends nothing, and the manager cannot reach it. The build changed two things in OB-152 by its own call. The delivery path has a second queue, A Human's own, and the source test that guards the manager's files forbids four more names. Built as TODO OB-179, with the checks for A Human in HUMAN_TODO OB-180 and the deferrals in TODO OB-182 to OB-207.

## OB-210 — How should a desk carry on when an allowance runs out, and what may cross the building on the way? — DECIDED 2026-11-12: a desk's own login may pass through the building's router to the same vendor and nowhere else, the first route is the Claude Code frontend, a new vendor carries on from the visible history, and the rest is left to the build

GitHub issue #8 asks for provider routing with a bounded fallback. Each person orders their own inference connections, and a desk moves to the next permitted one when an allowance is confirmed to be used up. It is a follow-up to OB-040 and amends one line of it.

Decided by A Human on 2026-11-12, before an unattended build of the issue. These four are new rulings for this issue:

- The credential boundary is "Pass-through plus app sign-in". A session's own login header may cross the app's loopback router on its way to the same vendor. It is never stored, never logged and never sent to another vendor. Application programming interface (API) keys for paid connections live in the Keychain. The app may also hold a sign-in of its own that A Human consents to (Sign in with ChatGPT). That needs A Human to register the app with OpenAI first, so it is built dormant. No client identifier is invented or hardcoded, and nothing signs in until one is supplied.
- The first route is the Claude Code frontend. Its chain may hold the Claude subscription (passed through as it is), an Anthropic API key or OpenRouter's Anthropic models as a paid fallback that is opted into, and an OpenAI API key reached through an experimental translation from Anthropic's Messages format to OpenAI's Responses format. Codex as a frontend, Gemini and Antigravity are not built. They appear in the compatibility matrix as unverified or unavailable, with the reason.
- Live probes are allowed for Claude Code and Codex on this machine's current login. They are a few one-line requests that record header names only, never values or content.
- Continuity across vendors carries on from the visible history. The next request goes to the new vendor with the full visible conversation and the tool results, hidden reasoning is dropped, and the desk shows that this happened. Handing a session over to the other command line tool is not built.

Already decided in the issue itself, as the build brief records, and not reopened:

- There are four tiers, Fable to Astra, Opus to Sol, Sonnet to Terra and Haiku to Luna, and A Human chooses the exact model identifiers.
- Paid fallback is opted into for each connection.
- Experimental adapters have a separate opt-in.
- Work and personal account contexts are kept strictly apart.

One earlier ruling is applied here. Commits stay local, and nothing is pushed until A Human approves (OB-059).

Calls the build made while A Human was away. Only the lines above are A Human's. Everything below is a call of the build brief's dispatcher, of the design or of the review. Each stands until A Human says otherwise, and A Human can veto any of them.

From the build brief, for the feature as a whole:

- Routing is off until A Human switches it on. With routing off a desk launches exactly as it does today.
- No model identifier, provider order or connection is seeded. A tier with no mapping for a connection is an error A Human sees, never a substitution.
- The router lives inside the app process and listens on the loopback address only, with a secret for each session passed through the child's environment and never on a command line.
- The issue asks for no new interface tests. The build wrote the ones the repo's coverage floor needs and no more.
- Nothing is proven against a paid account. Every route lands as experimental or unverified in the matrix, and the feature lands as built but unchecked.
- Only the dispatcher ran the live probes. A build agent never starts a session.

From the build brief, after recon and the live probes:

- The issue is built in three stages on one branch. Stage A is the preferences, the tier mappings, the selection policy, the failover state and its saved records, with the Routing group in Settings (TODO OB-208). Stage B is the listener, the relays to the same vendor, the routed launch, the live route commands and the status view (TODO OB-212). Stage C is the translation to OpenAI, the dormant Sign in with ChatGPT and the published matrix (TODO OB-214).
- Work and personal contexts are typed data. Every connection belongs to exactly one context, every floor is assigned one by its repo root, and selection, exhaustion and recovery never cross.
- Separate Claude and Codex logins for each context are not built, because discovery, usage, the hire options and the chat reader all read the real home folders (TODO OB-216).
- Preferences live in the settings file as a group of their own. Exhaustion records live in a file of their own, because a settings file that does not parse is reset to defaults and that must never reopen an exhausted connection. API keys live in the Keychain, are entered in Settings and never appear in a snapshot, a log or a file.
- The evidence that a Claude subscription is exhausted is one specific refusal, a 429 reply whose `anthropic-ratelimit-unified-status` header reads `rejected`. Anything else on a 429 is ambiguous and never counts as exhausted. No real exhaustion was captured, and Anthropic does not document the header.
- For the stages not built yet: the listener uses no async stack and takes one request for each connection, a session is told apart by a route identifier and authenticated by a secret compared in constant time, a routed child never gets an Anthropic key or token in its environment, a blocked chain is answered locally with a Hypertext Transfer Protocol (HTTP) 400 reply that says why and never with a 429 or a 529, no other connection is tried once any part of a reply has been relayed, and only Claude Code terminal desks the app starts are routed. The quality assurance reviewer, the manager, structured desks and anything started outside the app are not.
- No stop path is added (TODO OB-027), so a routed desk that outlives the app loses its model access. Stage B says so in the interface.
- The subscription relay is a connection kind of its own, so it can be switched off or removed without touching the rest. That is how the build answered the open call in OB-211.

From the design of Stage A. The first eight are the ones most worth A Human's eye:

- There is at most one Claude subscription connection in the whole building. The pass-through uses this Mac's one Claude login, and two contexts sharing it would break the separation of work and personal.
- A connection's kind and context cannot change once saved. It is removed and added again. A saved key must never reach another vendor or another context.
- A floor with no context, or with a context that no longer exists, is not routed. There is no machine default, because a default nobody chose is the fallback across contexts that the issue forbids (TODO OB-217).
- A session has one active connection, and exhaustion of it moves the whole session. A request never takes a detour to another connection, except while another request is running the one recovery check.
- An exhaustion record is never cleared by time, by an ordinary success or by anything read from the usage display. Only a recovery check that succeeds clears it, and only its own record. One check is let through for each reset time the provider reported. The router never sends a check of its own, and never moves a session back because a provider looks available (TODO OB-223).
- A running session keeps the copy of the chain it was opened with. A later change in Settings only narrows it, by a removed connection or a withdrawn opt-in, and never widens it (TODO OB-222).
- Efforts are names, not a closed list. On a connection that speaks Anthropic's Messages format an effort with no pair passes through unchanged. On a translated connection an effort with no pair refuses the request.
- A `routing` group in the settings file that does not parse, or breaks a rule, loads as off and empty, and the other groups are kept. Off and empty is the safe direction, since desks then launch as they do today.
- Exhaustion is scoped to the connection and to the whole of it, one tier or one model name. A subscription refusal marks only the scope of the request that was refused.
- The Keychain account of a key names the connection's kind and its identifier, so a leftover item can never be picked up by a connection of another kind.
- A recovery check is one of the desk's own requests let through. The router never invents a request.
- The router retries a passing failure at most once for each request, within 30 seconds of the first attempt, and never waits more than 5 seconds.
- Saving a key does not check it with the vendor. Saved means present, not accepted (TODO OB-220).
- The web side reaches routing through a seam of its own, `RoutingApi`, and a floor's context is read by its own app call and is not in the snapshot (TODO OB-225). Both follow from files that are at their line limit.
- The Routing group's hint says plainly that this build only saves the choices and routes nothing.
- Sign in with ChatGPT is a type that can never be tried, and the picker does not offer it.

From the review, after the design:

- When the `routing` group would load as off and empty, the settings file as it was is first copied to `settings.routing.invalid.json`, so the next save does not wipe what A Human had entered.
- A recovery check that is under way is not written to the records file as done, so quitting in the middle leaves the check due after a restart.
- A key cannot be saved for a connection that has just been removed. A Keychain refusal while a removed connection's key is deleted is still dropped (TODO OB-231).
- A failed write of the records file is offered again on the next write, and is not counted as saved.
- A probe's lease that has run out is treated as free everywhere, so a retry can start a new check.
- A floor's context goes back to what it was when saving it fails.

This is a follow-up to OB-040 and amends one line of it, and that amendment is A Human's own. OB-040 ruled that the building "never touches login tokens". A session's own login header may now pass through the building's router on its way to the same vendor. It is not stored, logged or sent anywhere else. The rest of OB-040 stands. The building runs each tool's own unmodified CLI and never an agent loop of its own. OB-011 and OB-152 say that nothing listens on a network port. That still holds after Stage A, which builds no listener, and the stage that builds one amends those words (TODO OB-212). A Human's ruling of 2026-10-01 that the app never stops a running agent (TODO OB-027) is not amended. One call is still open for A Human, on Anthropic's terms (OB-211). Stage A is built as TODO OB-208, with the checks for A Human in HUMAN_TODO OB-209 and the deferrals in TODO OB-212 to OB-232.

## OB-211 — May the building relay a desk's own Claude login to Anthropic before Anthropic has confirmed it in writing?

Raised by the build of GitHub issue #8 on 2026-11-12 (OB-210). A Human approved the pass-through before seeing the wording below.

The build brief records that Anthropic's legal page says developers "may not collect, store, or intermediate Claude.ai credentials or session tokens". Whether a loopback relay inside the user's own app, forwarding the user's own unmodified client to Anthropic, falls under that sentence is not settled by the documentation.

What the build did meanwhile. The subscription relay is a connection kind of its own, so it can be switched off or removed without touching the rest, and it is to be labelled in the compatibility matrix. Stage A holds only the preference. The relay itself comes with Stage B (TODO OB-212), and nothing is relayed until then.

**Options.**

- **Ask Anthropic first (recommended)** — keep the relay for A Human's own use on this Mac, labelled in the matrix, and get written confirmation from Anthropic before the building ships to anyone else with it.
- **Keep it as built** — treat a loopback relay of the user's own unmodified client as outside that sentence, and ship it without asking. Cost: the reading is the build's own, and nobody at Anthropic has confirmed it.
- **Remove the subscription relay** — drop the Claude subscription connection kind. A chain then starts at an API key, and the subscription is used only by desks that are not routed.

**Recommendation.** Ask Anthropic first. The relay stays a separate kind either way, so the answer changes nothing else in the design.

## OB-011 — How should assistant agents reach the building (MCP transport and auth)? — DECIDED 2026-10-01: Defer for now, later a proper server deployment of the app itself might happen…

Decided in Office Building on 2026-10-01: Defer for now, later a proper server deployment of the app itself might happen and then we’ll come back to this.

Amended on 2026-11-12 by A Human (OB-152). A floor's manager, which the
building itself starts, may reach the app over an owner-only Unix socket in
the app's data folder. Nothing listens on a network port, and the server for
assistant agents described below stays deferred.

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
