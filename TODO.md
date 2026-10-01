# TODO

Agent work. The founder's actions live in [FOUNDER_TODO.md](FOUNDER_TODO.md) and open calls in [FOUNDER_DECISIONS.md](FOUNDER_DECISIONS.md).

Verification of the Phase-1 scaffold (2026-09-29) against the UX sketch and the chat brief. The sketch defines the flow (UX), not the look; the look is recorded in DESIGN.md.

## UI

Fixed and verified in Chromium on 2026-09-29 (typecheck, 91 tests, build, screenshots at 1280×820 and 900×600, design-lint checker PASS):

- [x] Sidebar scoped to the floor (Decisions and TODOs now carry `projectId`).
- [x] Sketch flow: Decisions → questions by priority → one question → A / B / Other. Sections in the sketch's order on the rail.
- [x] Collapsed sidebar (rail) always visible on a floor; room objects open the same panels.
- [x] Discovered sessions raise their own window (`DiscoveryApi.focusSession`); app-spawned open the docked terminal; read-only show a transcript note.
- [x] "+" (hire desk) lets you pick the tool and name the task; the new hire walks in.
- [x] 2.5D cutaway building and isometric floor (CSS 3D rig).
- [x] Character poses per state, walking notes between colleagues, stuck agents lie on the floor (owner request).
- [x] Camera dive into a floor and back. Mock simulation keeps the office alive.
- [x] Terminal no longer overlaps the sidebar (separate layout columns).
- [x] Human words for states and tools, no emoji, name tags and hover cards for desks.

Open:

- [x] 3D scenes render correctly in Safari (Antek's screenshot, 2026-09-29). Playwright's WebKit build cannot render `preserve-3d`, so use real Safari or the Tauri app for WebKit checks.
- [x] Worker hover card was sliced by walls in Safari (a 3D plane at the worker's depth). It is now a flat overlay above the scene.
- [x] Sketch note between the sidebar and the question list reads "to be ticked off by me" (Antek, 2026-09-29): decisions are answered by the human, as built.
- [x] Hiring picks tool, model and effort (efforts are per model, as in Codex's models cache). Antek, 2026-09-29.
- [x] OB-004 — Hover slide on storeys could flicker at a storey's very edge (the room moved out from under the pointer). A transparent hit box now holds the hover in place while the room slides. Preventive fix: the flicker was not reproducible before it, and a Chromium sweep of 186 pointer positions shows 0 hover changes while the pointer is still. Owner: Claude.

## Backend (next)

- [x] Checked the CCC claims: see `docs/research/ccc-evaluation.md` (cited to CCC `main` at 20e2bfe, v5.35.0). The `port.txt` claim holds. The API has about 360 unversioned routes, no auth, and breaking changes in minor releases.
- [x] **Backend decided (Antek, 2026-09-29): our own Rust backend.** The repo will be public but never commercial. Port logic only from CCC v5.14.0 (MIT, keep its notice). Never copy from CCC `main`. Background: the locked decision "base it on CCC" rested on a false premise. Since 2026-07-28 CCC `main` is source-available, not open source: a revocable, non-commercial licence with no modified redistribution and no competing product (LICENSE §2 and §4, verified). Releases before 2026-07-28 stay MIT (LICENSE §6). v5.14.0 (`02bf3342`) is MIT, verified. Options: own Rust backend porting logic from v5.14.0 (recommended in the report), CCC `main` as a separate server for private use only, or ask Amir Fish for permission.
- [x] Rust compiles (cargo 1.98.1) with 27 unit tests. The old scaffold (in-memory mock, MCP stub, desk field) is replaced.
- [x] Rust mirror matches the TS contract (model.rs), with runtime validation of every payload on the TS side (src/api/wire.ts).
- [x] Claude Code discovery (milestone 1): liveness from `~/.claude/sessions/<pid>.json`, transcripts from `~/.claude/projects`, floors by git root, state rules and question scorer ported from CCC v5.14.0 (MIT), AskUserQuestion to the question queue, live updates by file watch plus a 5s tick. `cargo run --example scan` prints what it sees.
- [x] Hire options read live from `~/.claude/settings.json`, `~/.codex/models_cache.json` and `~/.codex/config.toml`.
- [x] Raise-window focuses the exact iTerm2 or Terminal.app tab by tty; other terminals are brought forward.
- [~] OB-005 — Hired agents run in in-app terminals (portable-pty + xterm.js, `src-tauri/src/pty.rs`) instead of Terminal.app. Their desks have full control, and a terminal follows its agent across `/clear`. The agent runs in the user's login shell (zsh, bash, sh or fish, otherwise /bin/zsh) with the launcher's session and terminal identity stripped. A launch that dies before the agent reaches its desk is reported as an error. The replay keeps the last 256 KiB without splitting control sequences and restores terminal modes. Two adversarial verifier rounds ran, and their tests are kept (`verifier_`, `verifier2_`). Residuals:
  - Not yet exercised in the running app window with a real hire (FOUNDER_TODO OB-018).
  - A launcher that keeps the agent as a child of a shell only returns at the 15 s cap.
  - `office://hire-failed` can fire falsely if an agent exits before any refresh binds it (a very fast `/exit`).
  - fish quoting is unit-tested only (fish is not installed).
  Owner: Claude.
- [ ] OB-016 — Terminal lifecycle: finished terminals are never released (two file descriptors and up to 256 KiB each, for the app's lifetime). An agent that ignores SIGHUP survives quitting the app. Keystrokes typed in the 500 ms drain after exit are dropped. The backlog's cols/rows are sent but unused. Found by the OB-005 verifier. Owner: Claude.
- [ ] OB-017 — The build warns about an 800 kB chunk, mostly xterm.js. Low value in a desktop app, where the bundle loads from local disk. Lazy-loading would make terminal creation asynchronous and needs the terminal tests reworked for it. Do it if startup feels slow. Owner: Claude.
- [~] OB-006 — Codex discovery: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` (session_meta, turn_context with model/effort, task_started/task_complete/turn_aborted), titles from `session_index.jsonl`, sub-agent threads dropped. Liveness: a session is running while a `codex` process holds its rollout open (lsof). Residual: threads started from the ChatGPT desktop app never show as running (its embedded `codex` holds no rollout open), and a fresh session is not shown running until its first turn creates the rollout. Owner: Claude.
- [~] OB-007 — Spend in dollars at API list prices (`src-tauri/src/discovery/pricing.rs`, sources cited there, read 2026-09-29). Claude is priced per message, covering 5-minute and 1-hour cache writes, cache reads, fast mode, US-only inference and web searches. Codex is priced per `token_count` growth at the model then in use, with long-context rates above 272K input tokens. Models with no public price (e.g. `gpt-reserve`) count as unpriced tokens, and the Spend panel says so. Tokens stay input + cache writes + output. Residuals: Codex Fast mode is not recorded in rollouts, so it is priced at standard rates. Owner: Claude.
- [x] OB-014 — Spend includes sub-agents: Claude Code's `<project>/<sessionId>/subagents/**/*.jsonl` (parsed incrementally like any transcript) and Codex sub-agent rollouts (walked up `parent_thread_id` to the top-level thread). Checked on real data: no message id repeats across sub-agent files or with the parent. A cold scan now reads every live or recent session's sub-agent files (1.17 GB for one real session, about 4 s at app start), then only appended bytes. Found during OB-007. Owner: Claude.
- [x] OB-008 — Transcripts are parsed incrementally from the last offset. A rewrite (new inode, shorter file, or changed bytes in the last 4 KiB, as when Claude Code removes a message in place) triggers a full re-parse. Owner: Claude.
- [x] A question asked in prose was lost when a system turn (task notification, peer message) followed it. The detector now reads the agent's latest reply to the human, so only the human's next prompt clears the question.
- [x] Decisions and TODOs come from each repo's own files (FOUNDER_DECISIONS.md or HUMAN_DECISIONS.md, DECISIONS.md, FOUNDER_TODO.md or HUMAN_TODO.md, TODO.md), falling back to the app's store when a repo has none. Answering a call and ticking an item write back into the file. See FOUNDER_DECISIONS.md OB-002.
- [x] OB-009 — Very long checklists (one real repo has 2,012 TODO items): done items fold away and the open list is capped at 200 with a count of the rest. Owner: Claude.
- [ ] OB-010 — MCP server for assistant agents (plan §12) and real agent-to-agent delivery. Blocked on OB-011 (transport and auth) in FOUNDER_DECISIONS.md. Owner: Claude.
- [x] OB-013 — The user can add a floor from the roof sign: pick a repo folder (native folder picker in the app, typed path in the browser) and it becomes a floor, pinned in the app's store, even with no sessions yet (Antek, 2026-09-29). Owner: Claude.
- [x] OB-012 — App icon re-rendered with transparent corners (every PNG is RGBA with alpha 0 at the corners). Owner: Claude.
- [~] OB-015 — Rust crate brought up to the Rust rules tightened on 2026-09-29. Done:
  - Edition 2024 and `rust-version = "1.98"`.
  - `.cargo/config.toml` with `global-min-publish-age = "7 days"`, which Cargo warns it ignores until Rust 1.100.
  - A `[lints]` table (clippy all deny, pedantic warn, and deny for unwrap/expect/panic/todo/unimplemented/dbg/as_conversions/allow_attributes) plus `clippy.toml` allowing unwrap/expect/panic in tests.
  - `cargo clippy --all-targets -- -D warnings` passes and rustfmt is applied.
  - `rust-toolchain.toml` pins 1.98.1 with clippy and rustfmt (installed 2026-09-30 with Antek's OK).
  - `unsafe_code` is `deny`, not `forbid`, because `pid_alive` needs `libc::kill`. That one site carries a reasoned `#[expect]`.
  - `missing_errors_doc` is allowed in the lint table, with the reason written there.
  Residuals:
  - The tools' JSON is still walked as `serde_json::Value` inside the parser functions (`discovery/claude_code.rs`, `discovery/codex.rs`, `options.rs`). No `Value` leaves a parser, but per-record typed serde structs would replace the field-by-field walking.
  Owner: Claude.
- [x] OB-019 — When a session has ended, its agent leaves the UI (Antek, 2026-09-30). The floor stays for 12 hours after its last activity, so its decisions and TODOs stay reachable. A floor with no activity in that time still needs Add a floor to stay. Owner: Claude.
- [x] OB-020 — A floor's light is grey when its agents are idle (not working and not asking) and green only when one is working (Antek, 2026-09-30). The "x/y active" tally now counts working agents too. Desks were already grey when idle. Owner: Claude.
- [x] OB-021 — Decisions written as checklist items (`- [ ] **Title** …`, as some repos write them) were not read, because only `## ID — Title` headings were parsed. (Antek, 2026-09-30). They are read now, with inline `(a)/(b)` options. Answering ticks the item and adds a `**Ruled <date>:**` line. Owner: Claude.
- [x] OB-022 — Decisions files survey across local repos (Antek, 2026-09-30). One repo's decisions were invisible, because its files are hyphenated (`FOUNDER-DECISIONS.md`, `FOUNDER-TODO.md`) and use `### F-NN · Title · urgency` entries under `## Open — …` groups with table options. Both are read now. Inline options only count after the word "Option", so lettered parts of a question are not offered as choices. Owner: Claude.
- [ ] OB-023 — A floor with open founder decisions disappears 12 hours after its last session, taking its decisions with it, unless it is added by hand. Consider keeping a floor for every repo whose decisions file has open calls. Owner: Claude.
- [~] OB-024 — A hired agent asking an `AskUserQuestion` showed as Idle (Antek, 2026-10-01). Claude Code writes an open question or permission prompt to the transcript only once it is answered. While it waits, the only sign is `"status":"waiting"` in `~/.claude/sessions/<pid>.json`, which is read now. The desk raises its hand and the decisions queue says "Waiting for you in its terminal". Tests pass. Still to check in the real app window. Owner: Claude.
- [~] OB-028 — Clicking a session started in Terminal.app or iTerm said "its terminal app could not be identified" (Antek, 2026-10-01). Regression from `ec3846c`: renaming the variable `ppid` to `parent` also rewrote the `ps -o ppid=,comm=` keyword, which macOS `ps` does not know, so every terminal lookup failed. Fixed in `focus.rs`, with a test that reads a live process's parent through the same call. Still to check in the real app window. Owner: Claude.
- [x] Mac-side Phase 2 list in README.md: superseded (2026-09-29). That section now points here, and every item it held is tracked above.
