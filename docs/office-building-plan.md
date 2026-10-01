# Office Building — plan

A macOS desktop app for managing coding agents & projects, visualised as an
office building: floors = projects, desks = agent sessions, animated employees
whose state reflects the live agent state.

Status: **the original plan, kept for background.** The app has since been
built; [`README.md`](../README.md) and [`TODO.md`](../TODO.md) describe it as it
is. The research (§0) looked for what already exists to reuse — the headline
finding is a near-exact open-source prior art (CCC) that could be our backend.

---

## 0. Research — existing tooling to reuse

Ethos: **curate & wire existing OSS, do
not reinvent.** Everything below was verified against live sources, not assumed.

### 0.1 The headline finding — `amirfish1/claude-command-center` (CCC)

An **actively-developed, open-source, near-exact prior art** for Antek's idea.
Verified: github.com/amirfish1/claude-command-center (updated ~weekly, has forks,
listed in "awesome-agent-orchestrators", live demo at ccc.amirfish.ai/demo).

What it already does (≈95% of our *backend*):
- **Unified session board across engines** — Claude Code, Codex, Cursor,
  **Antigravity**, Kimi, Grok, Devin, Droid, opencode, Kilo Code — however
  launched. **Reads each engine's on-disk state, so hand-started terminal
  sessions show up too.** (This is exactly our "auto-discovery", already solved
  for every tool on our list including the one we flagged "RESEARCH NEEDED".)
- **Live transcripts**, side-by-side panes, per-session input bars.
- **Full-text + semantic search** across all session history.
- **Group chats** = two+ sessions kept in sync on one goal (post once → all
  participants pinged) — i.e. **agent↔agent messaging already exists.**
- **Hierarchical "Flow" grouping** + a live "current sessions" triage band.
- Prior notes (to re-verify on the Mac): spawn via HTTP, per-engine model/effort
  ladders, cost/spend visibility, approval-request surfacing, discovery via
  `~/.claude/command-center/port.txt`, a `ccc-orchestration` Claude Code skill,
  phone-as-client, VS Code extension.

**Implication:** CCC = candidate **backend/engine**; office-building = a *sexy
desktop skin* over its data/HTTP API (or a fork). This collapses discovery,
transcripts, spend, approvals, and agent↔agent into "reuse", leaving us to build
mainly the **office metaphor UI**. Risk: coupling to a fast-moving 3rd-party repo
+ its license/API stability — must be verified on the Mac before committing.

### 0.2 Protocols (the standards-based alternative to CCC)

Mental model: **MCP = agent↔tools** (already used) · **ACP = agent↔client** ·
**A2A = agent↔agent**.

- **ACP (Agent Client Protocol)** — verified: github.com/agentclientprotocol,
  Apache-2.0, by Zed (Aug 2025), JSON-RPC 2.0 over stdio/HTTP. Editor renders
  chat/diffs/**permission prompts**; agent runs the loop, requests **file +
  terminal access**. Native in Zed & JetBrains; adapters for **Claude Code,
  Codex, Gemini CLI, opencode, Copilot CLI**. → our **discovery + terminal +
  the sidebar permission/question queue**, standards-based. (Note: "ACP" is an
  overloaded acronym — this is the *Client* one, not IBM's agent-comms one.)
- **A2A (Agent2Agent)** — Google, v1.0, the de-facto agent↔agent standard (IBM's
  ACP merged in). Agent Cards at `/.well-known/agent-card.json`, Task lifecycle,
  JSON-RPC/gRPC/HTTP. → the **agent↔agent messaging** feature if we don't take
  CCC's group-chat.

### 0.3 Terminal (don't reinvent the PTY)

- `tauri-plugin-pty` (Tnze, MIT) **or** `portable-pty` (wezterm crate) + xterm.js
  in the webview. Reference skills/apps: `yofabr/tauri-pty` (purpose-built for
  Tauri 2 + React + xterm), `volt-terminal`, `terminon`, `marc2332/tauri-terminal`.
  Use `@xterm/addon-webgl` for smooth rendering.

### 0.4 Other prior-art managers (patterns to borrow, not adopt wholesale)

Claude Squad (tmux+worktrees, TUI, AGPL) · Crystal→**Nimbalyst** (Electron GUI,
worktree-per-session, claude+codex+opencode, iOS companion) · vibe-kanban
(Rust+TS web kanban) · Conductor (native macOS, closed) · Opcode (Claude Code
command center) · agent-sessions (searches Codex/Claude history). **Common
pattern worth stealing: git-worktree isolation per session.** Our differentiator
= the **spatial office metaphor** + unified cross-tool + **manager sidebar**.

### 0.6 The key architectural fork (for Antek to decide)

**Option A — Reuse CCC as the backend.** Build the office UI over CCC's data/API
(or fork it). Fastest to a real, cross-tool, working app; agent↔agent + spend +
approvals mostly free. Cost: dependence on a fast-moving external repo; must
verify its API surface + license on the Mac; less control over the model.

**Option B — Build fresh on standards.** Our own Rust backend using ACP (for
discovery/terminal/permissions) + A2A (agent↔agent) + tauri-plugin-pty. More
work, but fully owned, standards-based, and matches the "wire OSS
primitives" ethos rather than depending on one app.

**Option C — Hybrid (the plan's lean).** Phase-1 UI on `MockApi` (browser-verifiable,
no dependency lock-in) while we **evaluate CCC on the Mac**. If CCC's API is
clean/stable → wire it as the Phase-2 backend (Option A). If not → fall back to
the ACP + tauri-pty stack (Option B). The `DiscoveryApi` seam (§6) makes the
backend swappable either way, so this decision stays cheap and reversible.

---

## 1. Stack

- **Shell:** Tauri v2 → native macOS `.app` (small, fast, real system access).
- **Frontend:** React 19 + Vite 6 (pinned `^6.4.3` per monorepo override) +
  TypeScript (strict, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`,
  `any` banned) + Tailwind CSS 4 + Framer Motion (animation) .
- **Backend (Rust, `src-tauri/`):** system access — session discovery, PTY
  spawn/stream, file watching, the agent message bus.
- **Terminal render:** `@xterm/xterm` in the webview (Phase 2). Phase 1 uses a
  styled mock terminal so the UI is fully browser-runnable with zero native deps.
- **Tests:** Vitest for the pure logic (selectors, status aggregation, search,
  discovery parsers). TDD on logic; components are the scaffold.

**Monorepo hygiene:** the workspace `build` script = `vite build` only (stays
cross-platform so `pnpm -r build` / `pnpm verify` never needs Rust). `tauri:dev`
/ `tauri:build` are separate Mac-only scripts. `src-tauri/target/` gitignored.
Note: this introduces **Rust** to a pnpm/uv repo — a deliberate, contained
addition Antek has signed off (desktop app).

---

## 2. The hard part #1 — session auto-discovery

Goal: automatically find running/recent agent sessions across tools and
normalise them into one `Session` model. Grounded storage locations (verified):

| Tool | Session store | State signal |
|------|---------------|--------------|
| **Claude Code** | `~/.claude/projects/<cwd-hash>/*.jsonl` transcripts; config `~/.claude.json`; process `claude` | newest jsonl mtime + last record role (assistant done vs awaiting user vs tool-running) |
| **Codex CLI** | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` (+ newer SQLite index). Records incl. `event_msg`, `response_item`, and notably `inter_agent_communication_metadata` | tail last JSONL record type/timestamp; process `codex` |
| **opencode** | **SQLite** at `~/.local/share/opencode/` (sessions/messages/projects); client/server w/ local HTTP API; `opencode debug paths` lists dirs | query the SQLite DB or hit the local server API; process `opencode` |
| **antigravity** | Google agentic IDE (VS Code/Windsurf-derived) — macOS storage under `~/Library/Application Support/` **(RESEARCH NEEDED)** | TBD — lowest priority |
| generic fallback | `ps`/`tmux`/`screen` scan for known binaries; cwd via `lsof` | process alive + cwd |

**Design:** a Rust `SessionSource` trait, one impl per tool. A background poller
combines file-watching (`notify` crate on the session dirs) with a periodic
`ps` sweep, emits normalised `Session` records to the frontend via Tauri events.
Each `Session`: `{ id, tool, projectPath, title, state, lastActivityAt,
pendingQuestions, spend }`. State is heuristic — transcripts are the most
reliable signal; "waiting-on-human" = agent emitted a question/prompt and is
blocked. Honest caveat: cross-tool state unification is best-effort.

**Projects (floors):** a project = git repo root (or cwd) of its sessions.
Sessions sharing a root cluster onto one floor. **Floor light** = worst state on
the floor: 🔴 red = errored or urgently blocked-on-human; 🟡 yellow = waiting /
needs attention; 🟢 green = healthy / idle / running fine.

---

## 3. The hard part #2 — live terminal attach

You cannot hijack another process's TTY. So:

- **App-spawned sessions:** run the agent inside a PTY we own (`portable-pty`)
  → stream bytes to xterm.js, send input back. Full interactive control. This is
  the primary, first-class path.
- **Discovered external sessions:** (a) if started under **tmux**, bridge via
  `tmux pipe-pane` / `capture-pane` (read, or read-write); (b) otherwise render a
  **read-only transcript view** from the jsonl/SQLite (a clean conversation view,
  not a raw TTY); (c) offer **"adopt"** = relaunch under our PTY for full control.

Recommendation for v1: full PTY for app-spawned + read-only transcript view for
discovered. Interactive control of arbitrary pre-existing sessions is a
stretch-goal via the tmux bridge.

---

## 4. The hard part #3 — agent ↔ agent messaging

A local Rust **message broker**: per-agent inboxes, agents post/read via a small
local socket (or an MCP tool we inject into app-spawned sessions). Codex already writes
`inter_agent_communication_metadata` — worth aligning with. Phase 4.

---

## 5. Sidebar (the manager's office) — from the sketch

Clicking the manager/CEO office opens the enlarged sidebar:
**Info · Decisions · Human-TODO · TODO · Spend**, plus a **priority-ordered
Question queue**. Each question renders as **A) / B) / Other: [input]** (exactly
the human-in-the-loop pattern) and the answer routes back to the agent (for
app-spawned / MCP-wired sessions). The sketch's note reads "to be ticked off by
me" (Antek confirmed, 2026-09-29): the decision queue is answered by the human.

---

## 6. Data model (TS, mirrored in Rust)

Discriminated unions, no `any`:
- `ToolKind = 'claude-code' | 'codex' | 'opencode' | 'antigravity' | 'unknown'`
- `SessionState = 'idle' | 'thinking' | 'running' | 'waiting-human' | 'error' | 'done'`
- `Project { id, name, path, sessions, light }`
- `Session { id, tool, projectPath, title, state, lastActivityAt, pendingQuestions, spend }`
- `Question { id, sessionId, prompt, options: {label}[], allowOther, priority }`
- `Decision`, `TodoItem`, `Spend { usd, tokens }`

Frontend consumes one typed **`DiscoveryApi`** interface with two impls:
`MockApi` (Phase 1) and `TauriApi` (Phase 2+, calls Rust). Clean seam so
swapping mock → real is a one-line provider change.

---

## 7. UI (building → office) mapped to the sketch

- **Search + active/total counters** at both levels (project list, per-project
  session list) — straight from the sketch's `X/Y` badges.
- **Building exterior:** stacked floors, one per project, each with a status
  light + name + active/total. Framer Motion hover/parallax.
- **Enter-floor transition:** click a floor → camera "rides up / zooms in" →
  open-plan office.
- **Office interior:** desk grid, one desk per session, each with an **animated
  employee** whose animation encodes state (idle breathing / typing / hand-up
  "waiting-human" / spinner "running" / red "error"). A **manager's office** in
  the corner. A **"+" desk** to add an employee (spawn a session).
- **Worker click:** opens the terminal panel for that session.
- **Manager office click:** opens the sidebar (§5).
- **Look & feel:** 2.5-D isometric (SVG/Canvas + Framer Motion) recommended over
  true 3D for speed + "sexy" without 3D-asset overhead; can upgrade to
  react-three-fiber later.

---

## 8. Phasing

1. **Phase 1 (browser-verifiable):** whole UI on `MockApi` — building,
   floor lights, enter transition, office with animated employees, manager
   office → sidebar + question queue, worker → mock terminal, search + counters,
   "+" add. Framer Motion polish. Verify: `tsc` + `vite build` + Vitest green.
2. **Phase 2 (Mac):** `TauriApi` → real **Claude Code** discovery + app-spawned
   **PTY terminal** (xterm).
3. **Phase 3:** sidebar intelligence real — question routing, spend, decisions,
   TODOs.
4. **Phase 4:** codex / opencode / antigravity discovery + agent↔agent bus.

---

## 9. Division of labour

- **Linux sandbox (first):** design + all cross-platform code — frontend, types,
  mock layer, Rust command *signatures* + per-tool discovery design, tauri.conf,
  READMEs. Verifies frontend (`tsc`/`vite build`/vitest).
- **Mac:** `pnpm install`; generate icons (`pnpm tauri icon logo.png`);
  `pnpm tauri dev`; implement/iterate the native Rust discovery + PTY (needs
  macOS to compile/run); grant the app Full-Disk / Accessibility perms.

---

## 10. Constraints & risks (honest)

- The Linux sandbox **cannot compile Rust or verify a macOS build** (no
  cargo) — Rust ships as reviewed-by-inspection source; the first native run
  happens on the Mac.
- Adds **Rust** to a pnpm/uv repo — contained via the build-script split above.
- **7-day dependency cooldown** (`minimumReleaseAge`) — pin to already-present /
  >7-day-old versions to keep the lockfile resolvable.
- **External-session terminal attach is fundamentally limited** (§3) — v1 is
  read-only for discovered sessions; full control for app-spawned.
- **antigravity storage unknown** — a research task, lowest priority.

---

## 11. Open decisions for Antek

0. **THE BIG ONE — backend strategy (§0.6): REVISED (Antek, 2026-09-29) →
   our own Rust backend.** CCC relicensed on 2026-07-28 to a revocable,
   non-commercial, source-available licence that bars competing products (see
   `docs/research/ccc-evaluation.md`). Office Building will be public and never
   commercial. We port logic only from CCC v5.14.0, the last MIT release, and
   keep its MIT notice. Nothing is copied from CCC `main`.
   *Superseded decision (2026-09-28):* base it on CCC and add our own bits. So: Phase-1 office UI on `MockApi`
   (browser-verifiable, no lock-in), Phase-2 backend = CCC (evaluate its API on
   the Mac, wire behind the `DiscoveryApi` seam), then extend with our own
   features (office metaphor, manager sidebar, our own touches). Option A/hybrid.
1. **DECIDED:** v1 terminal model — app-spawned agents get **full in-app control**
   (PTY + xterm). Discovered / hand-started sessions: **clicking the employee
   raises their real ORIGINAL terminal window** (native macOS window activation),
   NOT an in-app read-only transcript. Tricky bit (Mac side): mapping a discovered
   process → its owning terminal app + window/tab to raise it (AppleScript /
   Accessibility / `lsof`+window-list). Fallback if unmappable: read-only
   transcript from the jsonl/SQLite.
2. **DECIDED:** tool priority after Claude Code = **Codex next**, then the rest.
3. **DECIDED:** 2.5-D isometric.
4. **DECIDED:** **rich character animations** (idle/typing/hand-up/running/error
   PLUS walking-between-desks / reactions) — sexier; the mock layer already drives
   `SessionState`, so richer animation is additive.

## 12. NEW requirement — office-building as an MCP server (Antek, 2026-09-28)

The app **exposes its own MCP server** so personal-assistant agents can control the building programmatically —
not just via the GUI. Proposed MCP tools (mirror the `DiscoveryApi` + actions):
`list_projects`, `list_sessions`, `get_session`, `spawn_session` (add an
employee), `get_question_queue`, `answer_question`, `send_agent_message`
(agent↔agent route), `get_spend`, `get_decisions` / `add_decision`,
`list_todos` / `tick_todo`. Runs in the Rust backend (loopback), advertised so
assistant agents can attach. This makes the office both a **GUI and an
agent-controllable surface**, and dovetails with §4 (agent↔agent). Phase 3–4.
