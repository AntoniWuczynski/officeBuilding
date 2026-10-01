# FOUNDER TODO — actions only Antek can take

Agent work lives in [TODO.md](TODO.md). Open calls live in
[FOUNDER_DECISIONS.md](FOUNDER_DECISIONS.md). Office Building shows this file
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
