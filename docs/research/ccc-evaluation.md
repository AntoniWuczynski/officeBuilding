# CCC evaluation: can Office Building sit on claude-command-center?

- Repo: `amirfish1/claude-command-center` (CCC), branch `main`
- Commit read: `20e2bfe3f0aef9d3e725b9766a7d233be0d59acd` (2026-09-29T14:35:59Z, "feat(sessions): record the real caller of scripted sessions"), version `5.35.0` (`pyproject.toml:3`)
- Citations are `path:line` at that SHA unless another ref is named. Read via `gh api .../contents/<path>?ref=<sha>`, no clone.
- Our side: `src/api/DiscoveryApi.ts`, `src/types.ts`, `docs/office-building-plan.md` §0.

## 1. Licence

**Verified facts**

- Current licence is **not open source**. `LICENSE:1-3`: "Claude Command Center Software License / Copyright (c) 2026 Amir Fish. All rights reserved." `pyproject.toml:16` classifies it as `"License :: Other/Proprietary License"`. `NOTICE:4-6`: "source-available, free for non-commercial use, commercial use by permission."
- Non-commercial grant only, and revocable. `LICENSE:27-29`: "a limited, non-exclusive, non-transferable, revocable license to use, copy, and modify the Software for Non-Commercial Use". Non-Commercial Use is "use by an individual for private, non-commercial purposes ... where no compensation is received" (`LICENSE:14-17`). Commercial Use covers anything "intended to generate revenue ... including ... use in the development of products or services intended for sale" (`LICENSE:19-23`) and "requires a separate license" (`LICENSE:34-35`).
- Redistribution is **unmodified only**. `LICENSE:42-43`: "You may share unmodified copies of the Software ... for Non-Commercial Use." There is no grant to distribute modified copies at all. `LICENSE:45-52` then forbids: redistribution for Commercial Use, offering it "as a hosted or managed service", and "use the Software, or substantial portions of it, to build a product or service that competes with the Software".
- Relicensing history. The switch is commit `997edaac63` (2026-07-28T04:27Z, "chore(license): switch from MIT to source-available CCC license"). `LICENSE:62-66`: "Versions ... released before 2026-07-28 were published under the MIT License ... Those prior versions remain available under the MIT License". Checked at tags: `LICENSE@v5.14.0` (commit `02bf3342`, released 2026-07-27T23:00Z) is MIT, `LICENSE@v5.15.0` (released 2026-07-28T05:33Z) is already the CCC licence. **v5.14.0 is the last MIT release.**
- Third-party MIT islands (`NOTICE:11-17`): Grok engine support and liveness fixes "in server.py, tests/" and "Kilo Code engine and ACP adapter (ccc_acp.py and related)", both jaylfc, MIT.
- CLA (`CLA.md:27-35`) grants the maintainer an irrevocable licence to contributions and "the right to relicense ... to a source-available license like FSL or BSL". So anything we upstream becomes relicensable by him. It does not grant outsiders anything.
- `LICENSE-MIT` is a stock MIT text, "Copyright (c) 2026 Amir Fish" (`LICENSE-MIT:1-3`).

**What that means for Office Building (inference, not legal advice)**

| Option | Current `main` (CCC licence) | Last MIT release v5.14.0 |
|---|---|---|
| Depend on CCC as a separately installed process the user installs themselves, talk HTTP | Allowed for a private, non-commercial user. Our app contains no CCC code, so our repo's licence is ours. The risk is the "competes with the Software" clause (`LICENSE:50-51`): Office Building is a cross-engine session dashboard, which is exactly what CCC is. Whether an HTTP client of CCC "uses the Software to build" a competitor is arguable. The licence is also "revocable" (`LICENSE:28`) | Allowed, no conditions beyond MIT notice |
| Vendor (bundle CCC inside our .app or repo) | Unmodified copies only, non-commercial only (`LICENSE:42-43`). Any patch makes it a modified redistribution, which has no grant. A public GitHub repo containing CCC is redistribution | Allowed with the MIT notice |
| Fork and modify, public on GitHub | **Not permitted.** Modifying for private use is allowed (`LICENSE:28`), publishing the modified fork is not (only unmodified sharing is granted), and a competing product built from it is explicitly barred (`LICENSE:50-51`) | Allowed. But the fork is frozen at 2026-07-27 and cannot take any later upstream code (v5.15.0 onwards) without falling under the CCC licence |

**Flags**

1. The plan's §0 claim "open-source" is wrong for `main`. It is source-available and non-commercial.
2. If Office Building might ever be sold, offered as a service, or used at a for-profit employer, every option on `main` needs a commercial licence from Amir Fish (`LICENSE:34-35`).
3. "Competes with the Software" is the most dangerous clause for us specifically, because our product and CCC overlap on purpose.
4. "Revocable" means even the personal-use grant can be withdrawn.

## 2. Runtime and install

**Verified facts**

- **Process**: a single stdlib Python HTTP server, `server.py` (40,147 lines), plus an optional persistent worker `ccc_worker.py` that owns engine processes over a Unix-socket control plane (`server.py:100` imports `control_plane.ControlPlaneClient`, `server.py:39747-39751` computes `worker_owns_engines`). Server class is `socketserver.ThreadingMixIn + http.server.HTTPServer` (`server.py:39687-39689`).
- **Command and port**: `./run.sh`, `PORT=9000 ./run.sh`, `./run.sh --port 9001` (`server.py:11-14`). Default `PORT = int(os.environ.get("PORT", 8090))` (`server.py:19791`). `run.sh:988` ends with `exec "$PYTHON" "$HERE/server.py"`. `./run.sh --install-service` installs per-user launchd agents for dashboard and worker (README.md "From source" section).
- **Bind and network**: 127.0.0.1 by default, `CCC_BIND_HOST=0.0.0.0` is an opt-in escape hatch (`server.py:39840-39856`). Opt-in Tailscale "phone access" and federation between machines exist (`docs/federation.md`, `ccc_server/phone_access.py`).
- **Python**: `requires-python = ">=3.9"` (`pyproject.toml:6`), checked in `run.sh:765-766`. Runtime deps declared as none: "Zero runtime dependencies — stdlib only" (`pyproject.toml:27-30`).
- **But WatchTower is a hard import**: `server.py:789-800` does `import watchtower.queue` and on failure raises `ImportError("CCC's queue system requires the watchtower package ... it is a hard dependency with no standalone fallback")`. WatchTower is a separate repo by the same author (`amirfish1/watchtower`), installed by `scripts/install-watchtower.sh` into the same interpreter, needing Python 3.11+ (README.md "WatchTower comes with it"). The README contradicts itself: one passage says CCC "fails loudly at startup" without it ("What you get"), another says on older Python it "runs on a built-in fallback queue engine" ("WatchTower comes with it"). The code at this SHA raises.
- **Install paths**: curl installer cloning into `~/.ccc/claude-command-center`, a Homebrew tap (`amirfish1/ccc`), and a signed, notarised DMG `CCC.app` that "installs its local source into `~/.ccc/claude-command-center`" and self-updates via Sparkle (README.md "Quickstart", `docs/GETTING_STARTED.md:13-25`). The macOS app is a Swift launcher (`scripts/macapp/main.swift`) around the same Python server, bundle id `com.github.claude-command-center` (`server.py:3854`).
- **Discovery by other apps: the plan's claim is correct.** `ccc_server/fleet_jobs.py:1550-1585` `write_port_file()` writes a single line `http://<host>:<port>\n` to `COMMAND_CENTER_STATE_DIR / "port.txt"`, where `COMMAND_CENTER_STATE_DIR = Path.home() / ".claude" / "command-center"` (`ccc_server/paths.py:15`). Called at startup (`server.py:39865`). Caveats: skipped when `CCC_EPHEMERAL` is set (`fleet_jobs.py:1563-1565`), older builds wrote a bare port number (`federation.py:458-459`: "older builds wrote a bare port. Accept both"), and the bundled skill warns "port.txt can be stale — a now-dead secondary server may have left it pointing" (`skills/ccc-orchestration.md:13-14`). The `ccc` CLI resolves `--server URL > $CCC_SERVER > port.txt` (`ccc:52-54`, `ccc:70`).
- **Auth: none.** `server.py:29284`: "We have no auth — the trust model is 'loopback only'." Startup prints "This server has no auth. Anyone who can reach this port can run subprocesses on your machine" when bound off-loopback (`server.py:39854-39857`). The only guard is a CSRF Origin check on POSTs (`server.py:29281-29340`): a missing Origin is allowed, otherwise it must match `^https?://(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?$` or an allow-listed origin (`CCC_ALLOWED_ORIGIN`, `network.json allowed_origins`).

**Inference for us**

- A Tauri webview on macOS sends `Origin: tauri://localhost`, which does not match that regex, so direct `fetch` POSTs from our UI would get 403. Calls must go through our Rust side (no Origin header) or the user must allow-list the origin. Rust-side HTTP is the cleaner choice anyway.
- Any local process can drive CCC, including spawning agents. That is CCC's stated trust model, but a larger attack surface than our own Tauri command boundary.
- Depending on CCC means the user also gets WatchTower and a Python 3.11+ interpreter.

## 3. API surface (routes relevant to us)

All routes are hand-dispatched `if/elif` chains in `CommandCenterHandler.do_GET` (`server.py:25668`) and `do_POST` (`server.py:29426`). There is no route table, schema or OpenAPI file for the core API (the only OpenAPI file, `docs/kimi-kap/openapi.json`, covers the Kimi engine bridge). About 160 GET and 200 POST path checks exist. Request bodies are JSON, responses are JSON dicts (usually with `ok`). Shapes below are from the handler code, plus the bundled skill `skills/ccc-orchestration.md`, which is the closest thing to API docs.

**Sessions and live state**

| Method, path | Request | Response | Cite |
|---|---|---|---|
| GET `/api/sessions?repo_path=<abs>` | `include_old`, `since`, `state=waiting\|working\|idle\|ended`, `limit`, `fields=a,b` | bare JSON array of rows | `server.py:26647-26707`, params `server.py:38425-38470` |
| GET `/api/sessions?all=1` | `engine=`, `compact=1`, same filters | `{ok, sessions:[row], spawned:[...], count, conversations?}` (`conversations` is a legacy alias unless `compact=1`) | `server.py:26661-26687` |
| GET `/api/sessions?federated=1` | `limit`, `peers_only` | rows tagged `node_id, node_name, ref, stale` plus `nodes` | `server.py:26649-26659`, `skills/ccc-orchestration.md:112-116` |
| GET `/api/session/<id>` | none | `{ok, session_id, state, last_assistant_text, question_text, turns}` | `server.py:25868-25873`, `docs/attention-api.md:84-86` |
| GET `/api/sessions/events` (SSE) | none | baseline then deltas, one `data:` per change: `{id, state, question_waiting, needs_approval, ts}`, and a session leaving the live map emits `state:"ended"`, `: keepalive` every 20 s, internal poll every 1 s | `server.py:27169-27173`, `server.py:36509-36590` |
| GET `/api/events` (SSE) | `since=<seq>` or `Last-Event-ID`, `boot_id` | replayable envelopes `id: <seq>` from a bounded ring (capacity 512), `resync_required` when the cursor falls out | `server.py:27165-27168`, `server.py:36436+`, `ccc_server/events.py:1-40` |
| GET `/api/sessions/spawned` | none | CCC-owned spawns: `spawn_id, session_id, parent_session_id, engine, model, reasoning_effort, repo_path, cwd, spawned_at` | `server.py:26326`, `skills/ccc-orchestration.md:61-62` |

Row fields (from the seeded demo payload `docs/demo/api/conversations/all.json`, first row): `session_id, id, display_name, first_message, source, is_live, question_waiting, needs_approval, last_assistant_text, last_event_type, mtime, session_cwd, folder_label, folder_path, git_branch, sidecar_status, sidecar_in_flight, sidecar_ts, has_edit, has_commit, has_push, pr_state, ...`. Rows gain `state` and `ended_blocked` in `_apply_session_query_params` (`server.py:38442-38444`), `reasoning_effort` (`server.py:15918-15921`), `engine` for spawned rows (`server.py:22007`) and `model` when observed (`server.py:22357`). The skill warns rows have "no `name` or `title` field", use `display_name` (`skills/ccc-orchestration.md:50`).

**Transcripts**

| Method, path | Shape | Cite |
|---|---|---|
| GET `/api/conversations/<id>` | `after=<line>`, `tail=N`, `before=L`, returns `{events:[{type, line, ts, text, blocks}], last_line}` (keys from the demo payload `docs/demo/api/conversations/_id.json`) | `server.py:27589-27660` |
| GET `/api/conversations/<id>/stream?after=<line>` | SSE tail of new transcript lines | `server.py:27553-27557` |
| GET `/api/session/<id>/spawn-stream?replay=1` | SSE deltas for a spawned headless run | `server.py:27584-27588` |

**Spawn (model and effort: yes)**

- POST `/api/sessions/spawn` (`server.py:32751-33261`). Body: `prompt` (required), `repo_path` or `cwd` (required), `engine`, `model`, `reasoning_effort` (alias `effort`), `name`, `worktree`, `report_to`, `parent_session_id`, `node`, `key_profile`, `idempotency_key`. Engine and model resolved by `_spawn_request_engine_and_model` (`server.py:32771`), effort by `_spawn_request_reasoning_effort` (`server.py:32778`). Response: `{ok, session_id, spawn_id, parent_session_id, engine, repo_path, cwd, session_id_pending}` (`skills/ccc-orchestration.md:69`), 503 when the engine CLI is unavailable (`server.py:33238-33254`).
- An unknown effort is silently dropped, not rejected: "the spawn succeeds at that engine's default effort, so never infer from a `200` that your value was applied" (`skills/ccc-orchestration.md:67`). Only Codex rejects an unknown model with 400 (`skills/ccc-orchestration.md:66`).
- Claude spawns are **headless**, not terminals. `ccc_server/engines.py:5842`: "Spawn a headless Claude Code session", run with `--input-format stream-json --output-format stream-json --model ... --effort ...` (`ccc_server/engines.py:5368-5380`). Effort "is fixed at launch" (`engines.py:5846-5848`).
- Per-engine spawn routes also exist (`/api/sessions/spawn-codex`, `-cursor`, `-antigravity`, `-kilo`, `-opencode`, `-kimi`, `-grok`, `-hermes`, `server.py:33262-33791`).
- GET `/api/engines/models` (`server.py:28419-28423`, built at `server.py:8410-8733`): `{catalog:{<engine>:{default, supports_custom, byok_ready, models:[{id, label, cost_tier, cost_summary, max_context_tokens, reasoning_efforts, default_reasoning_effort, ...}]}}, engines:{<engine>:[ids]}, enforced:[], efforts_by_engine:{...}, kimi_thinking}`. Per-model effort rows are visible in the curated fallbacks, for example `claude-opus-5-5 ... "reasoning_efforts": ("low","medium","high","xhigh","max"), "default_reasoning_effort": "high"` (`server.py:7400`).

**Sending input**

- POST `/api/inject-input` (`server.py:35722-35892`): `{session_id, text, mode?: "send"|"steer"|..., peer_sender_sid?, idempotency_key?, force_terminal?, force_headless?, force_queue?}`. Routing goes through AppleScript keystrokes, tmux, `wt send`, UDS peer socket or headless resume (comment at `server.py:35855-35860`, `inject_input_via_keystroke` `ccc_server/session_graph.py:3950`, `inject_input_via_tmux` `session_graph.py:4031`). A result can be `ok` but only "queued" in CCC's durable queue (`server.py:35876-35879`).
- POST `/api/ask` (`server.py:36012-36044`): `{session_id, text, timeout_ms}` → `{ok, text, cost_usd, duration_ms, num_turns}` (README "Orchestration skill" example).
- POST `/api/inject-esc` (`server.py:35993`) interrupts.

**Questions and approvals waiting on a human**

- GET `/api/attention?scope=live|all` (`server.py:25837-25867`): feed of sessions needing the human, each `{kind, priority, session_id, name, where, did, insight, next_step, question_text?, soft_block?}` (`server.py:37796-37990`). Kinds: `question_blocked` (1), `stale_tool_call` (1), `pending_tool` (1), `sidecar_waiting` (2), `soft_block` (2), then git/backlog kinds 3-7.
- GET `/api/question?session_id=` (`server.py:25895-25911`): `{ok, pending, session_id, nonce, questions:[{header, question, multiSelect, options:[{label, description, preview}]}]}` (option shape `hooks/pre-tool-use.py:186-213`). POST `/api/answer-question` `{session_id, answers:[{index, text}]}` (`server.py:35926-35946`). **Only for CCC-spawned headless Claude sessions** whose PreToolUse hook is installed and blocking (`server.py:630-642`: "headless `claude -p` auto-declines AskUserQuestion ... for CCC-spawned sessions the PreToolUse hook blocks"). The answer is delivered by denying the tool with the answer as the reason (`hooks/pre-tool-use.py:305-308`).
- POST `/api/claude/permission` `{session_id, decision:"accept"|"decline"}` (`server.py:35457-35483`): drives the interactive TUI by a System Events keystroke, "macOS + live-TTY only" (`ccc_server/session_graph.py:4531-4556`).
- POST `/api/codex/approval` `{session_id, decision}` (`server.py:35431-35456`), POST `/api/acp/approval` `{harness, session_id, request_id, option_id}` (`server.py:35484+`).
- GET `/api/interrupt-asks`, POST `/api/interrupt-asks/resolve` (`server.py:27151-27159`, `35982`) for interrupt-approval banners.

**Group chats and agent-to-agent**

- POST `/api/group-chat/create` (aliases `/api/coordinate`, `/api/group-chats/create`) `{topic, session_ids, include_human}` → `{ok, chat_path, id, uuid, results}` (`server.py:35086`, `skills/ccc-orchestration.md:86-89`). Chats are Markdown files under `~/.claude/group-chats/` plus JSON sidecars.
- POST `/api/group-chat/post` `{id|path, text, session_id?, name?, emoji?, host_node?}` (`server.py:35146-35182`). POST `/api/group-chat/add`, `/nudge` and a dozen `/api/group-chats/*` admin routes (`server.py:35113-35431`).
- GET `/api/group-chats/active` → `{ok, chats}`, GET `/api/group-chat/read?id=` (`server.py:28917-28960`).
- Direct one-to-one messaging is `/api/inject-input` with `peer_sender_sid` (`skills/ccc-orchestration.md:71`). There is no stored per-message record of "A sent B text" exposed as a list: not found (searched GET routes for `message`, `peer`, `inbox`).

**Spend and cost**

- GET `/api/session/<id>/usage` (`server.py:26984-27055`) calls `extract_session_usage` (`ccc_server/morning_launch.py:2055-2068`), which walks the transcript and returns `{latest_input_tokens, peak_input_tokens, total_output_tokens, total_input_tokens, total_cache_creation_tokens, total_cache_read_tokens, model, context_limit, cost_usd, reasoning_effort, cost_breakdown_usd}` (`cost_usd` rounded at `morning_launch.py:2307`). The handler adds `cache_adjusted_tokens` and Opus-5 baseline-equivalent tokens (`server.py:27025-27052`). One call per session, one transcript walk each. `cost_usd` is API list-price, not what a subscription user pays (README: "allocated subscription dollars next to the API list-price equivalent").
- GET `/api/usage/current`, `/api/usage/pace`, `/api/plan-usage`, `/api/weekly_usage` (`server.py:26312-26324`, `26892`): plan windows and pace, not per-session dollars. GET `/api/byok/usage?days=30` for BYOK ledgers (`server.py:28463`).

**Focus or raise a terminal window**

- POST `/api/jump-terminal` `{session_id}` or `{tty, terminal_app}` (`server.py:36068-36083`) → `focus_terminal_by_tty` (`ccc_server/session_graph.py:4612-4734`): AppleScript tab-level focus for iTerm2 and Terminal.app, app-level `activate` only for Ghostty and others, "macOS-only". TTY comes from `session_live_status`, which reads `~/.claude/sessions/<pid>.json` (`session_graph.py:2899-2904`), so tab-precise focus is Claude-first.
- POST `/api/launch-terminal` `{session_id, cwd?, terminal_app?, stop_headless?}` opens a new terminal running the resume command, or focuses the existing one, and refuses while a headless process owns the session (`session_graph.py:3676-3688`).

**Search**

- GET `/api/search-history?q=&cwd=&since=&limit=&semantic=1` (`server.py:28473-28496`): BM25 over `ccc_server/session_fts`, optional local Ollama embeddings.
- GET `/api/search-recall-sessions` (`server.py:28497`), and `/api/memory/*` recall routes.

**Decisions and TODOs (for completeness)**

- GET/POST `/api/decision-inbox` (`server.py:26614`, `34945-34960`): CCC's own "decision cards" produced by a scanning loop or external producers, `{source, source_id, title, detail, severity, options}` (README "Decision Inbox"). GET `/api/memory/decisions/extracted` returns "explicit user rulings found by the nightly scan" (`server.py:28624-28637`).
- TODO.md entries are ingested read-only as backlog cards (README "Backlog" bullet, `server.py:22292`). No route to tick or edit a TODO: not found (searched all GET/POST path literals for `todo`, `backlog`, `parking`).

## 4. Discovery

**Engines and on-disk sources (verified)**

- README "Engine support" table: Claude Code, Codex, Cursor, Antigravity, Kilo Code, Kimi Code, OpenCode, Devin can be spawned and ingested. GitHub Copilot CLI, VS Code Copilot Chat and Grok CLI are "ingested **read-only**". Also Hermes, Aider, Droid and Pi modules are adopted (`server.py:25333-25352`) but not in the README table.
- Claude Code: transcripts `PROJECTS_ROOT = Path.home() / ".claude" / "projects"` (`server.py:3607`, README: "first-class JSONL (`~/.claude/projects/*.jsonl`)"). Liveness from Claude's own registry `~/.claude/sessions/<pid>.json` (`ccc_server/session_graph.py:2899-2904`: "The registry gives us an authoritative pid↔session mapping written by Claude Code itself") plus CCC hook sidecars in `~/.claude/command-center/live-state` (`server.py:20277`).
- Codex: `CODEX_SESSIONS_ROOT = Path.home() / ".codex" / "sessions"` (`ccc_server/paths.py:41`), dated subfolders `~/.codex/sessions/YYYY/MM/DD` (`server.py:14222`), `CODEX_HOME` honoured (`server.py:7710`). Live turns can also stream through the Codex app-server bridge (README "Engine support", Codex row).
- Others per README table: Cursor `~/.cursor/projects/` (partial), Antigravity `~/.gemini/antigravity/brain/`, Kilo `~/.local/share/kilo/kilo.db`, Kimi `~/.kimi-code/sessions/`, Devin local SQLite.

**Hand-started sessions: yes.** README line 12: "attaches to every **Claude Code**, **Codex**, ... session on your machine, however you launched it". Mechanically, ingestion scans the engines' own stores above, not a CCC registry, so a session started in a terminal appears. Precise live state for Claude, however, depends on CCC's hooks (`pre-tool-use.py, post-tool-use.py, notification.py, stop.py, ...`, `server.py:20280-20282`), which since this version are installed into the user's agent config "only after they approve each item" (`server.py:39735-39736`). Codex hooks go to `~/.codex/hooks.json` (`server.py:20283-20285`).

**State derivation (verified, `_session_state_label`, `server.py:37719-37776`)**

Four states, top-down:
1. `ended`: `not is_live` ("a dead process is 'ended' regardless of how its last turn read").
2. `waiting`: `question_waiting` or `needs_approval` (formal AskUserQuestion or permission marker) or `_detect_soft_block(c)`, a scored prose heuristic on the last assistant text (trailing `?` +3, phrases like "want me to", "should I" +2 each, threshold 3, `docs/attention-api.md:16-34`).
3. `working`: `pending_tool` or `sidecar_in_flight` or fresh sub-agents or `sidecar_status == "active"` within `_WORKING_GAP_WINDOW = 120` s (`server.py:9396`).
4. `idle`: live, nothing in flight.

Liveness uses a sidecar window `_SIDECAR_LIVE_WINDOW = 1800` s (`server.py:9387`). `ended_blocked` flags a session that died while waiting (`server.py:37779-37793`). There is no `error` state and no `thinking` versus `running` split: not found in `_session_state_label`. The nearest error signal is the attention kind `stale_tool_call` ("tool call may be stuck", `server.py:37880-37910`) and usage-limit fields (`_attach_usage_limit_resume_fields`, `server.py:26674`). There is no `done` distinct from `ended`, apart from `ended_blocked == false`.

**Inference**: the soft-block heuristic will produce false positives and negatives, and it is English-phrase based. It is still more than we have. Our `error`, `thinking` and `done` states would have to be derived by us from row fields (`last_event_type`, `stale_tool_call`, `ended_blocked`).

## 5. Stability

**Verified facts**

- **Stated policy: public.** `AGENTS.md:87`: "`/api/*` endpoints are the stable surface external tooling (agent hooks, the browser UI, pkood integration) binds to. Treat them like public API". Additive changes are fine, renaming or removing a field or changing a shape "is a **breaking change** — major version bump" (`AGENTS.md:88-90`). `AGENTS.md:81`: "Major for breaking `/api/*` contracts". `CHANGELOG.md:6` claims Semantic Versioning. `docs/attention-api.md:92-93`: "`/api/*` changes are additive ... no rename/remove".
- **Practice does not match the policy.** Endpoints and fields were removed in minor releases:
  - 5.33.0 (2026-09-13): "Removed the Model Advisor feature ... gone from the backend (`model_advisor.py`, `/api/model-advisor*` routes)" (`CHANGELOG.md:432`).
  - 4.6.0 (2026-06-03): removed `/api/watcher`, `/api/watcher/start`, `/api/watcher/stop`, "the `watcher_enabled` field on `/api/config`" (`CHANGELOG.md:2391-2393`), and `/api/logs`, `/api/logs/<issue>` (`CHANGELOG.md:2401`).
  - A "Breaking:" repo-context change (`/api/repo/switch` now errors, `400 repo_required` otherwise) shipped under 3.x (`CHANGELOG.md:3216`).
  - Conversely 5.0.0 was a major bump for a model feature ("Claude Fable 5 support", `CHANGELOG.md:1938-1941`), not an API break.
- **No API versioning.** Routes carry no version prefix, except federation (`/api/federation/v1/sessions`, `server.py:28995`). No schema, no OpenAPI, no contract tests for external consumers found (the `tests/` folder has 468 files, not inspected individually). Documentation of shapes is spread across the README, the orchestration skill and a few `docs/*.md`. The skill itself warns of shape traps ("rows have **no `name` or `title` field**", `skills/ccc-orchestration.md:50`) and legacy aliases (`conversations` alias kept "for API compatibility", `server.py:26683-26686`).
- **Cadence.** 69 tags from `v0.1.0` (2026-04-23) to `v5.35.0` (2026-09-29), five major versions in about five months, roughly three releases a week (from `gh api .../releases`). About 1,165 commits to `main` in the 30 days before 2026-09-29 (commits API pagination, `since=2026-08-30`). Single maintainer (`LICENSE:12`, `CLA.md:22-23`). `server.py` alone is 40,147 lines, with ongoing extraction into `ccc_server/` (`ccc_server/__init__.py:1-9`).
- Repo stats at time of reading: 175 stars, 20 forks, created 2026-04-13 (`gh api repos/amirfish1/claude-command-center`).

**Inference**: treat the HTTP API as semi-stable. Core routes we need (`/api/sessions`, `/api/sessions/spawn`, `/api/inject-input`, `/api/ask`, `/api/engines/models`, `/api/sessions/events`) are used by CCC's own skill and CLI, so they are the least likely to break. Anything else can disappear in a minor release. We would need to pin a CCC version and run our own contract tests against it.

## 6. Mapping to our `DiscoveryApi` and data model

Our contract: `src/api/DiscoveryApi.ts:23-57`, `src/types.ts:5-137`. "Fit" is my judgement: good, partial, none.

**Methods**

| Our method | CCC provides | Fit | Gap we build |
|---|---|---|---|
| `getSnapshot()` | GET `/api/sessions?all=1&compact=1` (`server.py:26661-26687`) plus `/api/attention?scope=live` (`server.py:25837`), `/api/question?session_id=` per waiting session (`server.py:25895`), `/api/session/<id>/usage` per session (`server.py:26984`) | partial | Fan-out and join into `OfficeSnapshot`, map `ToolKind` from `source`/`engine`, group rows into `Project`s, map states. Cost: N+1 calls, and the skill warns list calls "can exceed 15s" (`skills/ccc-orchestration.md:39`). |
| `subscribe(listener)` | SSE `/api/sessions/events` emits `{id, state, question_waiting, needs_approval, ts}` only (`server.py:36509-36590`), and SSE `/api/events` replay stream (`server.py:36436`) | partial | Events carry flags, not rows, so we re-fetch on change and debounce. No push for new sessions' metadata, spend or chat messages. |
| `hireOptions()` | GET `/api/engines/models`: per-engine `default`, `models[{id, label, reasoning_efforts, default_reasoning_effort}]`, `efforts_by_engine` (`server.py:8410-8733`, row example `server.py:7400`) | good | Map engine keys to `ToolKind`. Effort `label`/`description` are not provided, only ids: we supply text. Engines without a ladder give `[]`. |
| `spawnSession(projectId, {tool, title, model, effort})` | POST `/api/sessions/spawn` `{prompt, repo_path, engine, model, reasoning_effort, name}` (`server.py:32751-33261`) | partial | `prompt` is required ("missing prompt" 400, `server.py:32917`), our `HireRequest` has none. Spawn is headless (`ccc_server/engines.py:5842`), so our `control: "full"` (in-app PTY) is not what CCC gives. We would run our own PTY or accept headless plus transcript. Unsupported effort is silently dropped, so we must confirm via `/api/sessions/spawned`. `session_id` may be pending. |
| `answerQuestion(questionId, answer)` | POST `/api/answer-question` `{session_id, answers:[{index, text}]}` for relayed AskUserQuestion (`server.py:35926`), POST `/api/claude/permission` accept/decline (`server.py:35457`), `/api/codex/approval`, `/api/acp/approval`. Soft blocks are answered by POST `/api/inject-input` | partial | CCC has no question ids, so we mint them (session id + `nonce`). Structured options exist only for CCC-spawned headless Claude with the relay hook installed. Hand-started Claude questions arrive as `question_text` prose with no options, and permission answers go by keystroke into the terminal (macOS, live TTY). Our `QuestionAnswer` `option` maps to `{index}`, `other` to `{text}` or inject. |
| `tickTodo(todoId)` | Nothing. TODO.md is read-only backlog (`server.py:22292`, README "Backlog") | none | Build entirely: parse and write TODO.md (or our own store). |
| `sendAgentMessage(from, to, text)` | POST `/api/inject-input` `{session_id: to, text, peer_sender_sid: from}` (`server.py:35722`, `skills/ccc-orchestration.md:71`), plus group chats `/api/group-chat/create`, `/post` (`server.py:35086`, `35146`) | good for delivery | No stored log of one-to-one messages, so our `messages` list must be kept by us (or modelled as two-person group chats and read via `/api/group-chat/read`). |
| `focusSession(sessionId)` | POST `/api/jump-terminal {session_id}` (`server.py:36068-36083`) | good on macOS | Tab-precise only for iTerm2 and Terminal.app, app-level activate for Ghostty and others (`ccc_server/session_graph.py:4629-4712`). TTY lookup is Claude-registry based (`session_graph.py:2899`). |

**Data**

| Our data (`src/types.ts`) | CCC source | Fit | Gap |
|---|---|---|---|
| `Project {id, name, path, sessionIds}` | GET `/api/repo/list` (`server.py:28728`), row `folder_path`, `folder_label`, `session_cwd`, also Flow objects (`docs/flow-workspace.md`, not inspected) | partial | Choose the grouping key (repo root versus worktree cwd) and mint ids. |
| `Session.state` (`error`, `waiting-human`, `running`, `thinking`, `idle`, `done`) | row `state` in `ended`, `waiting`, `working`, `idle` (`server.py:37719-37776`) | partial | No `error`, no `thinking` versus `running`, `done` ≈ `ended && !ended_blocked`. Derive the rest from `stale_tool_call`, `last_event_type`, usage-limit fields. |
| `Session.model`, `Session.effort` | row `model` when observed (`server.py:22357`), row `reasoning_effort` (`server.py:15918-15921`), and `/api/session/<id>/usage` fills both | good | Nulls for engines that do not report. |
| `Session.control` | derivable: headless spawned (`/api/sessions/spawned`) versus live TTY (`session_live_status`) | partial | Our mapping logic. |
| `Session.lastActivityAt` | row `mtime` | good | none |
| `Session.spend {usd, tokens}` | `/api/session/<id>/usage` `cost_usd` and token totals (`ccc_server/morning_launch.py:2055-2068`) | good, but per-session call | List-price USD, not subscription cost. One transcript walk per session. |
| `Question {prompt, options, allowOther, priority, askedAt}` | `/api/question` (`questions[].question/options[].label`), attention feed `priority`, `question_text` | partial | Options only for relayed AskUserQuestion. `askedAt` not provided (use row `mtime`). |
| `Decision {title, detail, by: ai\|human}` | `/api/decision-inbox` cards (CCC-generated prompts to decide), `/api/memory/decisions/extracted` ("explicit user rulings", `server.py:28624`) | partial, different concept | Our decisions log is ours to build. Extracted rulings could seed it. |
| `TodoItem` | TODO.md backlog rows, read-only | none for write | Build. |
| `AgentMessage` | group chat Markdown files, `/api/group-chat/read` | partial | One-to-one message history is ours to build. |

## 7. Verdict

**Verified facts that drive the decision**

1. `main` is not open source. It is a revocable, non-commercial, source-available licence that only allows sharing unmodified copies and forbids building "a product or service that competes with the Software" (`LICENSE:27-29`, `42-52`). Plan §0.1 calls it "open-source", which is out of date since 2026-07-28.
2. v5.14.0 (`02bf3342`, 2026-07-27) is the last MIT release. Its `server.py` (63,588 lines, no per-file header, covered by the MIT `LICENSE@v5.14.0`) already contains `_session_state_label`, `_detect_soft_block`, `/api/sessions/events`, `/api/sessions/spawn`, `/api/engines/models`, `/api/inject-input`, `/api/answer-question` and `/api/jump-terminal` (`server.py@v5.14.0:58697`, `58653`, `57772`, `55076`, `52253`, `57283`, `57367`, `57510`), and WatchTower was optional there (`server.py@v5.14.0:507-515`).
3. The API is broad and covers discovery, state, spawn with model and effort, input, approvals, group chats, spend and window focus (§3). It is also undocumented as a schema, unversioned, has no auth, and has removed endpoints in minor releases (§5).
4. It does not cover our TODOs, our decisions log or a one-to-one message log, and its spawns are headless rather than an in-app PTY (§6).
5. Running it means a Python 3.9+ server, WatchTower on Python 3.11+, launchd agents, and CCC's hooks installed into the user's Claude and Codex configs (§2, §4).

**The three options in plain words**

| | A. Run CCC as a separate server, talk HTTP | B. Fork CCC | C. Own Rust backend |
|---|---|---|---|
| Speed to a working demo | Fastest. Most of `getSnapshot`, `hireOptions`, `spawnSession`, `focusSession` work in days | Slow. We inherit a 40k to 64k-line Python monolith | Slowest at first. Claude Code and Codex discovery plus state is the bulk |
| Licence | Personal, private use only. Grey area under the "competes" clause. Revocable. Needs a commercial licence for anything revenue-linked | `main`: publishing a modified fork is not permitted. v5.14.0: MIT, fine, but frozen at 2026-07-27 and cut off from all later fixes | Clean. We own it |
| Control and stability | Low. Semi-stable API, removals in minor releases, ~1,165 commits a month upstream, single maintainer | Full, but we maintain Python inside a Tauri/Rust app | Full |
| Fit with our design | State model, questions and control mode do not match 1:1. TODOs, decisions, messages still ours | Same as A, but editable | Built to our types |
| Install burden on the user | Heavy: CCC, WatchTower, Python, hooks consent | Heavy (we ship Python) | Light: one app |
| Security | No auth on a loopback server that spawns agents | Same unless we add auth | Our Tauri command boundary |

**Recommendation (inference, based on the facts above)**

Build our own Rust backend (option C), and use CCC as a reference, not a dependency. Specifically:

1. **Reopen the locked decision.** It rested on "open-source" and "≈95% of our backend". The first is false for `main`. The second holds only for a private, personal tool, and even then about half of our `DiscoveryApi` (`tickTodo`, decisions, message log, in-app PTY, error and done states) is ours to build regardless.
2. **Port ideas, and where useful code, only from v5.14.0 (MIT)**, with the MIT notice kept. The state machine (`_session_state_label`), the soft-block scorer, the Claude liveness trick (`~/.claude/sessions/<pid>.json`) and the AppleScript focus snippets are small, well-commented and exist in the MIT release. Do not copy code from any version after 2026-07-28.
3. **Scope the Rust backend to Claude Code and Codex first** (`~/.claude/projects`, `~/.claude/sessions`, `~/.codex/sessions`). Other engines later, one adapter each, which is how CCC grew too.
4. **Optional, personal only:** if the owner wants a quick live demo before the Rust backend exists, a throwaway `CccApi` implementation of `DiscoveryApi` behind the existing seam can talk to a locally installed CCC through the Rust side (Tauri's `tauri://localhost` Origin fails CCC's CSRF check, `server.py:29306`). Keep it out of any public build and do not treat it as the product backend.
5. **If the owner still wants option A for the product**, the precondition is a written licence or permission from Amir Fish (`LICENSE:34-35`, contact in the licence) that covers a public repo and a competing UI. Without that I would not build on it.

What would change my view: a relicence back to a permissive licence, or explicit written permission, plus a versioned, documented API. Then option A becomes the fastest sensible path.

