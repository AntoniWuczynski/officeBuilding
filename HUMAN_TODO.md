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

- [ ] OB-098 — Check the Linear connector (TODO OB-097) in the installed app
      (`pnpm app:install`). No agent may call Linear, touch the Keychain or
      drive the Tauri window, so nothing here has run against the real
      things. You need a Linear workspace you can make a key in.
      1. Before anything else, open a floor that has a `TODO.md`. Its items
         are all still there, each with a small `TODO.md` badge, and ticking
         one shows at once.
      2. In Settings, Connectors, paste a key you know is wrong and save it.
         The message should say Linear did not accept the key, and nothing is
         saved. If it starts "Linear said" instead, note the wording (TODO
         OB-118).
      3. Make a Linear key with write access (Linear's Settings, Account,
         Security & Access) and save it. The status line names your account.
         Note whether macOS asks anything about the Keychain.
      4. On a floor's Info panel, use the Linear row to connect a team. Within
         a minute, issues assigned to you are on the cork board, and
         unassigned issues and issues labelled `agent` are on the whiteboard,
         after the file items. Each has a `Linear · ENG-123` badge that fits
         its row, and each board has a status line with a Re-check button.
      5. Click a badge. The issue opens in your browser.
      6. Tick a Linear item. The question appears in the row itself, and
         Cancel changes nothing. "Mark as Done" ticks it in the app and moves
         it to Done in Linear, and it leaves the board about a minute later.
      7. Save a read-only key and tick an item. The tick comes back off and a
         message gives Linear's reason.
      8. Turn Wi-Fi off for two minutes. The boards keep their Linear items
         and say how old they are. Turn it back on and press Re-check.
      9. Reinstall the app (`pnpm app:install`) and open a connected floor.
         Note whether macOS asks to allow access to the Keychain item, and
         whether "Always Allow" lasts through the next reinstall. The app is
         ad-hoc signed, so it may not (TODO OB-117). While the prompt is
         open, press Re-check. The window should stay usable.
      10. Confirm the key is nowhere but the Keychain. It is not in
          `settings.json` or `store.json` in the app's data folder, and the
          first few characters of the key do not appear in `ps -axww`.
      11. On the Info panel, change the floor to another team. The first
          team's issues leave the boards at once. Disconnect, and the Linear
          items and the status line go, while the floor stays pinned.

- [ ] OB-123 — Check the QA desk (TODO OB-122) in the installed app
      (`pnpm app:install`), started from Finder or the Dock and not from a
      terminal. No agent may post to GitHub, start a real reviewer or drive
      the Tauri window, so nothing here has run against the real things.
      Quality assurance (QA) posts real reviews from your own GitHub account,
      so use a pull request of yours in a repo where a test review does no
      harm. You need `gh` signed in (`gh auth status`).
      1. Before switching anything on, open a floor. There is no QA desk, no
         stamp and no reviewer, and its Info panel has a "Quality assurance"
         row with the switch off. In Settings, the Quality assurance group
         lists Claude Code's models.
      2. Switch the floor on. A line in the row asks first, and Cancel
         changes nothing. After "Turn on" the floor is pinned, a desk with a
         magnifying glass stands in the front-left corner clear of the other
         desks, and its tag reads "QA". Look at it on a floor with one row
         of desks and on one with three. Within a minute the row names the
         repo and says how many pull requests are open. If it says `gh` was
         not found, the app did not find it from Finder (TODO OB-136).
      3. Open a pull request there, or push to one, and wait about 10
         minutes. A reviewer walks in and sits at the corner desk, and
         `git worktree list` in your repo shows one more worktree, under the
         app's data folder. Your own checkout and branch are unchanged.
      4. Click the reviewer. Its terminal shows no "trust this folder"
         prompt, provided you have used Claude Code in this repo before. Ask
         it to run a command or edit a file. It should refuse both.
      5. Wait for it to finish. If it is still there after 20 minutes and is
         then ended with no review, its report hook did not fire
         (TODO OB-135).
      6. The review appears on the pull request from your account, as a
         comment and not an approval, headed "Office Building Quality
         Assurance", with inline comments on changed lines. GitHub may not
         allow a comment review on your own pull request at all. If it
         refuses, the Info panel lists the review as "Not reviewed" with
         GitHub's reason after three tries (TODO OB-134). Check that the
         review is there once and not twice (TODO OB-146).
      7. Afterwards the reviewer has left, its desk is not on the sign-out
         sheet, `git worktree list` no longer shows the worktree, and
         `ps -ax` shows no leftover `claude` process for it.
      8. The stamp is on the desk's tag and on the floor's nameplate in the
         building view. It reads "QA passed" or "QA notes", is drawn and not
         an emoji, and fits the nameplate.
      9. In the Info panel the review is listed, and clicking it opens that
         review on GitHub in your browser.
      10. Push another commit. The stamp clears and one new review arrives.
          Wait through two more checks and confirm the same commit is not
          reviewed again.
      11. Label a pull request `no-qa`, and open another as a draft. Push to
          both. Neither is reviewed.
      12. Switch the floor off while a reviewer is working. The reviewer is
          ended, the worktree goes, and nothing is posted.

- [x] OB-051 — Put the launch video in the README with an inline player. Done
      2026-10-11: Claude uploaded `brag-output/brag.mp4` through a new-issue
      comment box in Chrome (nothing posted) and put the `user-attachments`
      link at the top of the README. The video stays out of git
      (`brag-output/` ignores itself). A re-cut needs a fresh upload and a new
      link.
