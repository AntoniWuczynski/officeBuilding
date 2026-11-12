# HUMAN TODO — actions only A Human can take

Agent work lives in [TODO.md](TODO.md). Open calls live in
[HUMAN_DECISIONS.md](HUMAN_DECISIONS.md). Office Building shows this file
on the officeBuilding floor as Human TODO; ticking an item there writes `[x]`
here.

- [ ] OB-003 — Decide whether to ask agents to use AskUserQuestion for
      decisions (e.g. a line in `~/.claude/CLAUDE.md`). Questions asked in
      plain prose reach the Decisions queue without A/B options; questions
      asked through AskUserQuestion arrive with their real options.

- [ ] OB-018 — In the app (`pnpm tauri dev`), hire a cheap agent (Claude, haiku,
      low effort, a small task) on a floor and check it: the desk appears,
      clicking it opens the in-app terminal, typing works, and `/exit` ends
      it. Agents can't drive the Tauri window, so only you can run this.

- [ ] OB-058 — Check the alerts and Settings (TODO OB-057) in the installed
      app (`pnpm app:install`). Agents can't drive the Tauri window, the app
      is ad-hoc signed, and the notification plugin always reports permission
      as granted, so only you can tell whether anything arrives.
      1. Cmd+, opens Settings on the building view and on a floor, and Escape
         closes it. If nothing opens, an app menu item is taking the shortcut.
      2. On the Setup floor the fuse box stands beside the toolbox, its tag
         does not overlap the toolbox's tag, and clicking it opens Settings.
      3. Press "Test in 3 seconds", then switch to another app. The sound
         plays, the Dock icon bounces once, and a notification appears under
         the name Office Building. This is the step most likely to fail. If
         no notification appears, look at System Settings, Notifications,
         and any Focus mode first.
      4. Clicking the notification brings Office Building forward.
      5. With a real agent, let it finish a turn while another app is in
         front (one alert). Then let it finish one with its panel open and
         the app in front (no alert).
      6. Set the cooldown to 30 and let two agents stop within it. The second
         alert arrives 30 seconds after the first, merged with a third if
         one stops in the meantime.
      In `pnpm tauri:dev` the notification is attributed to Terminal by the
      plugin's design, so only the installed app shows the real name and icon.

- [x] OB-051 — Put the launch video in the README with an inline player. Done
      2026-10-11: Claude uploaded `brag-output/brag.mp4` through a new-issue
      comment box in Chrome (nothing posted) and put the `user-attachments`
      link at the top of the README. The video stays out of git
      (`brag-output/` ignores itself). A re-cut needs a fresh upload and a new
      link.
