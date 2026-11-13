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

- [ ] OB-151 — Check the floor manager (TODO OB-150) in the installed app
      (`pnpm app:install`). No agent may hire a real manager, open the app's
      real socket or drive the Tauri window, so nothing here has run against
      the real things. The manager is a Claude Code session on your plan, so
      pick a cheap model for it in Settings, in the Manager group, first. Use
      a repo where notes to its desks and one test entry in its
      `HUMAN_DECISIONS.md` do no harm.
      1. Before hiring, open a floor. The chair in the manager's office is
         empty and the desk carries a "Hire a manager" sign that fits it. The
         rail has a Manager section, and Settings has a Manager group that
         lists Claude Code's models. The Setup floor has neither the sign nor
         the section.
      2. Press "Hire a manager". The manager appears in the office, the floor
         is pinned, its first reply sums up the floor, and the panel says when
         its tools last answered. No prompt asks you to approve a server. This
         is the step most likely to fail. If the panel keeps saying it has not
         used the building's tools, open its terminal and note what Claude
         Code says about the `office` server (TODO OB-166). On a floor where
         Claude Code has never run, the manager stops at the "trust this
         folder" prompt until you accept it in its terminal.
      3. In the app's data folder, `ls -la ipc` shows the folder as
         `drwx------` and `m.sock` as `srw-------`. `ps -axww` shows no long
         run of hex characters on the manager's command line. The token is not
         in `settings.json`, in `store.json` or in the `mcp.json` under
         `roles`. `ps eww` does show it in the manager's environment, which is
         TODO OB-164.
      4. Ask the manager to run a command and to edit a file. It has no tool
         for either. From another Claude Code session, ask Claude to message
         the manager. The message is refused.
      5. With two idle Claude Code desks on the floor, ask the manager to
         split `TODO.md` between them. Each desk gets one note, typed and
         sent once, starting "Note from this floor's manager". The manager
         walks each note out, and the panel's log lists them as delivered. Do
         the same with a Codex desk whose hooks you have trusted. If a note is
         typed and not sent, or sent twice, note which tool (TODO OB-166).
      6. While a desk is busy, have the manager send it a note. The note
         waits, the desk carries a sticky note with a 1 and a "managed" mark,
         and the panel says why it waits. Type a message to that desk
         yourself. The note is replaced and is not sent later.
      7. Put a desk at a permission prompt and have the manager send it a
         note. Nothing is typed until you have answered the prompt.
      8. Type half a line in a desk's terminal and leave it. A note for that
         desk waits, however long you leave it, until you press Enter or
         Ctrl-C there (TODO OB-167).
      9. Have a note sent to a desk you started by hand in a terminal. It
         waits, and the panel says nothing is typed into a desk started
         outside the building.
      10. Switch Paused on. A send is refused and waiting notes stay. Switch
          it off and they go out.
      11. Ask the manager to hire a desk. It is refused until you switch "May
          hire desks" on. Then ask for four. Three are hired and the fourth is
          refused, and the line under the switch counts them.
      12. Ask the manager to escalate a question with two options. It is on
          the memo pile and at the end of the repo's `HUMAN_DECISIONS.md` as
          an `MGR` section, and the manager's desk light is yellow until you
          answer it.
      13. Type `/exit` in the manager's terminal. The chair is empty, the sign
          is back, and the panel keeps the log under "Earlier". The Paused
          and "May hire desks" switches are as you left them when you hire
          again.
      14. If hiring says the app's data folder path is too long for a socket,
          note the message. It can happen with a long user name.

- [ ] OB-180 — Check the chat view (TODO OB-179) in the installed app
      (`pnpm app:install`). No agent may start a real session or drive
      the Tauri window, so none of this has run against the real tools.
      Every chat desk is a real session on your plan, so pick a cheap
      model in the hire dialog. Use a repo where a few test edits do no
      harm.
      1. Open Settings with Cmd+,. There is an Agents group with View, set to
         Terminal. Switch it to Chat, quit the app and open it again. It still
         says Chat.
      2. Open the hire dialog. The "Chat view" switch is on, because it starts
         at the setting, and switching it off applies to that one hire. Pick
         Antigravity. The switch is greyed out and says "Chat is not
         available: chat for Antigravity is not built yet, so it runs in the
         terminal."
      3. Hire Codex with "Chat view" on and a task that edits a file. The
         panel opens as a chat with no terminal. The reply streams, a tool
         call is a card that folds, the edit is drawn as a diff with removed
         lines in red and added lines in green, and an approval can be
         answered in the chat. Send a message while the turn runs. It waits
         above the box and goes out when the turn ends. Press "Stop turn"
         during a turn. The turn stops and the desk stays. Do the same with
         opencode or Cursor if you use them.
      4. Hire Claude Code with "Chat view" on and a task that edits a file.
         The panel opens as a chat at about 60% of the floor's height. Replies
         appear message by message, not word by word, and an Edit shows as a
         diff. Note how far the chat lags behind the terminal, and say whether
         that is acceptable (TODO OB-198).
      5. When that desk is idle, send it a message from the box. Flip to
         Terminal. The message was typed once and sent once. Then send a
         harmless slash command, such as `/help`, and note what Claude Code
         does with it (TODO OB-203).
      6. While the desk is busy, send two messages, and press Enter twice on
         the second. Two messages wait, not three, each with "Waiting: it is
         busy." Withdraw one. The other is typed once when the turn ends.
      7. Type half a line in the desk's terminal and leave it. Flip to Chat
         and send a message. It waits and says the terminal holds something
         you typed, until you press Enter or Ctrl-C there.
      8. During a turn, press "Stop turn". Claude Code stops the turn, as if
         you had pressed Escape once, and the session stays. Press it twice
         quickly on another turn. Nothing else happens: no rewind menu opens
         and no draft is cleared. Then send a message. It goes through, though
         it may wait up to about a minute (TODO OB-187).
      9. Put the desk at a permission prompt. The chat shows a card, "Claude
         Code is waiting for your answer", with one button, "Answer in the
         terminal". The button flips the panel and answers nothing. Back in
         the chat, "Stop turn" is refused and says why, and a message you send
         waits until you have answered the prompt.
      10. Flip between Chat and Terminal several times during a turn, with a
          message waiting. Both show the same session, and the waiting message
          is still there and is sent once.
      11. Hire Claude Code with "Chat view" on and no task, and send a message
          at once. Note how long it waits before it is typed (TODO OB-187).
      12. Drag the chat's top edge. Close the panel and open a terminal desk.
          The terminal has kept its own height, and the chat opens at the
          height you chose. At the smallest window size the floor's head is
          still reachable.
      13. Open the chat of a Claude Code desk with a long history. The top
          says "Earlier messages are not shown here.", scrolling stays smooth,
          and a long output is folded behind a "Show all" button.
      14. Ask an agent to reply with a Markdown link, an image and a line of
          raw HTML. The link and the image show as text with their addresses,
          nothing can be clicked, nothing is loaded and the HTML is not drawn.
      15. On a floor with a manager, let a manager's note wait for a busy desk
          and then send that desk a message yourself. The note is replaced,
          and your message is the one typed.
      16. With a message waiting, type `/exit` in the desk's terminal. The
          desk leaves and the message goes with it, without a word (TODO
          OB-195).
      17. A question for you, not a check. The build brief said to include
          Antigravity's chat through `agy acp`, and the build cut it, because
          nobody could run it unattended. Say whether the cut stands. If you
          want it built, it needs one live probe of `agy acp` with you present
          (TODO OB-184).

- [ ] OB-209 — Check the Routing group and a floor's account context (TODO
      OB-208), and then a routed desk (TODO OB-212), in the installed app
      (`pnpm app:install`). Steps 1 to 18 are for the first of three
      stages, which saves your choices. They send nothing to a provider and
      cost nothing. Steps 19 to 33 are for the second stage, which routes.
      They use your real Claude login, and the steps marked "Paid" spend
      money on a real key. No agent may drive the Tauri window, write to
      the real Keychain, start a routed desk or use a paid account, so none
      of this has run against them.
      1. Open Settings with Cmd+,. Routing is the last group. Its switch,
         "Route Claude Code desks through the building", is off. The hint
         under it says what off and on do, and ends by saying that a routed
         desk loses its model access if the building quits.
      2. Add two account contexts, "Work" and "Personal". Rename one. Quit
         the app and open it again. Both are still there.
      3. In Work, add a Claude subscription connection. Its card says that
         requests go to Anthropic untouched, and it has no key field and no
         model rows. In Personal, the Kind picker now greys out Claude
         subscription and says the Claude login on this Mac can belong to
         one context only. Sign in with ChatGPT is greyed out too, with its
         own sentence.
      4. In Work, add an "Anthropic API key" connection, where API stands
         for application programming interface. "May spend money" is off.
         Type any made-up key of letters and digits and press Save. Nothing
         checks it with Anthropic. Note whether macOS asks about the
         Keychain. The status reads "An API key is saved in your Keychain.
         It has not been checked with the provider." Quit, open the app
         again and open that card. It still says saved.
      5. Confirm the key is nowhere but the Keychain. It is not in
         `settings.json` or `store.json` in the app's data folder. In
         Keychain Access, the item's account starts with `routing-key.`
         and names the connection's kind.
      6. Reinstall the app (`pnpm app:install`) and open that card again.
         Note whether macOS asks once for each connection that has a key.
         The app is ad-hoc signed, so it may (TODO OB-230).
      7. Press "Remove key". The status reads "No API key is saved for this
         connection.", and the item is gone from Keychain Access.
      8. Add an "OpenAI API key" connection. It has both switches, "May
         spend money" and "Allow this experimental route". Type a model
         name in a tier's first field. In that tier's efforts field type
         `xhigh high` and leave the field. It says "Write each pair as
         asked=sent, separated by commas." and saves nothing. Type
         `xhigh=high` and it saves.
      9. Under Model tiers, type a model name under Opus and the same name
         under Sonnet. The second is refused with a line that starts "Could
         not save that" and gives the reason, and the field goes back to
         what was saved.
      10. With two or more connections in a context, use Move up and Move
          down. The numbers in the cards' headings follow, the buttons at
          the ends are greyed out, and the order is still there after a
          restart.
      11. Save a key on a connection, then press "Remove connection". It
          asks in a row inside the card and says the saved key is removed
          too. After Remove, the key's item is gone from Keychain Access
          (TODO OB-231).
      12. Remove a context that has connections. The row names the context
          and how many connections go with it.
      13. Look at the whole group in the real window. The cards are paper
          with an ink border and no shadow, a tier's two fields sit under
          its label without spilling out of the plate, and at the smallest
          window size the plate still scrolls to the end. Tab through a
          card. Every field, switch and button can be reached, and a card
          opens and closes from the keyboard.
      14. Close Settings and open a floor's Info panel. There is an "Account
          context" row with a picker that starts at "Not set" and the hint
          "Desks on this floor start as they do today." Pick Work. While
          routing is off, the row says it has no effect yet. Restart the
          app. The floor still shows Work.
      15. Remove the Work context in Settings, leave Personal, and open the
          Info panel again. The picker is back at "Not set".
      16. Leave routing off, or pick a floor with no account context, and
          hire a Claude Code desk there. It starts exactly as it did before,
          and its panel has no "Route:" strip (TODO OB-215).
      17. A question for you, not a check. HUMAN_DECISIONS OB-210 lists the
          calls the build made while you were away. Read the ones marked as
          most worth your eye and say which you veto.
      18. A second question. HUMAN_DECISIONS OB-211 is open. It asks whether
          the building may relay a desk's own Claude login to Anthropic,
          given a sentence in Anthropic's terms that the build found after
          you approved it.
      19. A routed desk. In Work, leave only the Claude subscription
          connection in the order. Switch routing on, give a floor the Work
          context and hire a Claude Code desk there. Open its panel. Under
          the panel's head there is a strip with a small lamp that reads
          "Route: No request yet". Give the agent a task that reads a file
          and runs a command. The strip changes to the connection's name,
          and the agent works as it does on a desk that is not routed.
      20. Press Details on the strip. It lists the desk's order, and the
          connection reads "Serving this desk". A line says what the last
          request asked for and that it was sent as asked. The last line
          warns that the desk loses its model access if Office Building
          quits. The strip pushes the terminal down and covers nothing, and
          Tab reaches Details. Flip the desk to the chat view. The strip is
          there too.
      21. Note anything a routed desk does differently from an unrouted one.
          Look at the context meter, since token counts are now an estimate
          (TODO OB-234). Look at the plan's usage meters and `/usage` (TODO
          OB-256). Remote Control is off on a routed desk, as Claude Code
          documents. Write down any feature that fails outright (TODO
          OB-235).
      22. The secret is not on show. Scroll to the top of the desk's
          terminal. The launch line starts with `env -u ANTHROPIC_API_KEY`
          and holds `ANTHROPIC_BASE_URL=http://127.0.0.1:` with a port. It
          does not hold `ANTHROPIC_CUSTOM_HEADERS`. In another terminal, run
          `ps -axo command | grep x-office-building-route`. It finds only
          the `grep` itself.
      23. The listener is local only. Run `lsof -nP -iTCP -sTCP:LISTEN |
          grep 127.0.0.1` and find the app's line with the port from step
          22. No line shows that port with `*:` in front. Then run `curl -i
          -d '{}' http://127.0.0.1:<port>/s/x/v1/messages`. The answer is a
          400 whose sentence starts "Office Building did not recognise this
          desk".
      24. Longer work (TODO OB-254). On a routed desk, run a task that
          starts helpers, and one long enough to compact the conversation,
          or type `/compact`. Both carry on. Route a second desk on another
          floor whose context has a different order, and work on both at
          once. Each strip shows its own route.
      25. End a routed desk with `/exit` and resume it from the sign-out
          sheet. It is routed again, and its strip starts at "No request
          yet". If the floor has a manager that may hire, a desk the manager
          hires there is routed too.
      26. Switch routing off in Settings while a routed desk runs. The desk
          carries on and keeps its strip. A desk hired after that is not
          routed. Switch routing on again.
      27. Paid. An Anthropic key (TODO OB-247). Under Model tiers, type the
          model names your desks ask for. In Work, add an "Anthropic API
          key" connection below the subscription with a real key that
          belongs to one workspace, switch "May spend money" on and type its
          model name for each tier. Hire a new routed desk, open Details and
          press "Switch to this" on the key connection. The strip now names
          it, and Details says "Switched from" the subscription to the key
          "by you", with a sentence about hidden reasoning. Send one short
          request. It is answered, and the usage shows in Anthropic's
          Console. If it is refused, copy the sentence the agent's terminal
          shows.
      28. Paid. Withdraw the permission. With that desk still on the key
          connection, switch "May spend money" off for it in Settings.
          Details now reads "Not used" for it, with the reason. The next
          request is answered in the agent's terminal with a sentence that
          starts "Office Building:" and gives the reason. The agent stops at
          once and does not keep retrying, and nothing is billed. Switching
          the permission back on does not reach this desk, because a running
          desk's order is never widened (TODO OB-222). Start a new desk.
      29. Paid. OpenRouter (TODO OB-243, OB-244). Add an OpenRouter
          connection with a real key, "May spend money" on and its model
          names. On a new routed desk that has already had one answer from
          the subscription, switch to it. Note whether OpenRouter accepts
          the request at all, since the building tells it to use no fallback
          provider and to drop no parameter. Details says hidden reasoning
          is removed from every request sent through OpenRouter. Then switch
          back to the subscription and send a request. Note whether it is
          answered, refused or answered after a pause (TODO OB-246).
      30. A real used-up allowance (TODO OB-213). This one cannot be staged.
          The next time your Claude plan reaches its limit while a routed
          desk works, with a paid connection below it in the order, note
          what happens. The desk should move to the next connection by
          itself, and Details should say "Moved from" the subscription
          "because the allowance" was used up, with the time the provider
          says it resets. If the desk does not move, copy what the agent's
          terminal shows. The rule for this was read from a replayed reply,
          never from a real one. Later, after the reset time, press Retry on
          the subscription and note whether the desk can be switched back.
      31. Quit the app while a routed desk is working (TODO OB-238). Check
          with `ps` whether the desk's process is still there. If it is, it
          can no longer reach a model. Open the app again and note how that
          desk shows. It has no strip, because routes are not saved.
          Starting it again from the sign-out sheet routes it afresh.
      32. A question for you, not a check. HUMAN_DECISIONS OB-210 now lists
          the calls the second stage made. One of them narrows how your
          fourth ruling is applied. Hidden reasoning is removed only on the
          way to OpenRouter, and is left in on the way to Anthropic's own
          API, because Anthropic's documentation says not to strip it there.
          Say whether that stands.
      33. A second question. OB-011 and OB-152 used to say that nothing
          listens on a network port. A listener on the loopback address now
          exists once a routed desk has been launched, and both entries
          carry a note. It follows from your pass-through ruling, and the
          details are the build's. Say whether the note stands.
          HUMAN_DECISIONS OB-211, on Anthropic's terms, is still open, and
          the relay it asks about now exists.

- [x] OB-051 — Put the launch video in the README with an inline player. Done
      2026-10-11: Claude uploaded `brag-output/brag.mp4` through a new-issue
      comment box in Chrome (nothing posted) and put the `user-attachments`
      link at the top of the README. The video stays out of git
      (`brag-output/` ignores itself). A re-cut needs a fresh upload and a new
      link.
