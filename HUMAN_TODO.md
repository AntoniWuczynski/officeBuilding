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

- [ ] OB-074 — Check "Open in" (TODO OB-073) in the installed app
      (`pnpm app:install`). Agents can't drive the Tauri window, and no test
      starts a real editor, so only you can tell whether the right folder
      opens.
      1. Open a hired desk's panel. Its dark header has an "Open in Cursor"
         button that fits the header, and a click opens the floor's folder in
         Cursor. Cursor has no command on this Mac, so this goes through the
         app alone (`open -a`) and is the step most likely to fail.
      2. The arrow beside the button offers VS Code, and picking it opens the
         folder in VS Code through its `code` command.
      3. Put two agents on one floor, one in the main checkout and one in a
         worktree. Each desk opens its own checkout, and the worktree desk's
         tooltip names its branch.
      4. Right-click a worker you started by hand in a terminal. The small
         menu appears at the pointer, is not cut by the walls, and macOS's
         own context menu does not appear. Escape and a click elsewhere
         close it.
      5. Let an agent start a sub-agent in a worktree. A click on the figure
         at its tiny desk opens the menu, and the menu opens that worktree.
      6. Cmd+Shift+O opens the desk whose panel is open. With no panel open,
         Tab to a worker and press it again. If nothing happens, an app menu
         item is taking the shortcut.
      7. The button on a sign-out sheet row opens the folder that session
         worked in, and the one in the Info panel opens the floor's folder.
      8. In Settings, Editor, pick VS Code as the default. The buttons now
         say "Open in VS Code". Zed and Devin Desktop are listed greyed out
         as not installed.
      9. A floor whose folder has a space in its name opens.
      10. Remove a worktree an agent is working in (`git worktree remove`),
          then click its button. The message says that worktree is gone and
          nothing opens.
      11. Only if you install them: Zed, Devin Desktop, or Cursor's `cursor`
          command (TODO OB-082). Check that each opens a folder, and note
          Devin Desktop's app name in `/Applications` (TODO OB-083).

- [x] OB-051 — Put the launch video in the README with an inline player. Done
      2026-10-11: Claude uploaded `brag-output/brag.mp4` through a new-issue
      comment box in Chrome (nothing posted) and put the `user-attachments`
      link at the top of the README. The video stays out of git
      (`brag-output/` ignores itself). A re-cut needs a fresh upload and a new
      link.
