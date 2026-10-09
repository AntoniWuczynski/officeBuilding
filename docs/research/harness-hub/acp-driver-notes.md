# ACP driver notes (2026-10-08)

Files: `acp.rs` (process, JSON-RPC plumbing, handshake, `Driver`), `acp/wire.rs`
(typed messages), `acp/transcript.rs` (message → `Event` mapping, no process),
`acp/tests.rs` (recorded lines + two `#[ignore]`d live tests).

## Sources used
- agentclientprotocol.com: `/protocol/initialization`, `/session-setup`,
  `/prompt-turn`, `/tool-calls`, `/session-config-options`, `/v1/authentication`.
- `schema/v1/schema.json` + `meta.json` (schema 1.24.1) from
  github.com/agentclientprotocol/agent-client-protocol.
- Unstable `session/set_model` / `models`: from `schema.unstable.json` at the
  parent of commit "Remove unstable session model API (#1325)", because Cursor still sends `models`.
- opencode.ai/docs/acp, cursor.com/docs/cli/acp.
- The `agent-client-protocol` crate is async, so I wrote a small typed JSON-RPC layer on std threads instead. No crates added.

## Methods used
Client → agent: `initialize` (protocolVersion 1, fs read/write false, terminal
false, clientInfo), `session/new` (`cwd`, `mcpServers: []`),
`session/set_config_option` (config option with `category: "model"`),
`session/set_model` (fallback when only `models` is offered), `session/prompt`,
`session/cancel` (notification).
Agent → client: `session/update` (agent_message_chunk, agent_thought_chunk,
tool_call, tool_call_update, plan; the rest ignored), `session/request_permission`,
`$/cancel_request` (answered -32800, approval resolved). Any other request
(fs/*, terminal/*, cursor/ask_question, cursor/create_plan, …) gets -32601.

## Decisions
- `authenticate` is never sent. While Cursor was logged out, `authenticate
  {methodId: cursor_login}` took 16 s, returned `{}` and **left the Cursor CLI
  logged in as the user** (it must have gone through a browser sign-in). That is
  the tool's own business. So `session/new` → -32000 becomes a notice: "sign in
  with the tool itself", plus the agent's own auth-method descriptions (Cursor:
  "Run 'agent login' first", opencode: "Run `opencode auth login`"). The
  process is then ended with that reason in `Exited.message`.
- opencode advertises `authMethods` even when no sign-in is needed, so their
  presence alone is not taken to mean sign-in is required.
- Spawned as `$SHELL -l -c "exec <tools::binary> acp <opts>"` (same PATH lookup as
  `tools::installed`, no `-i` so nothing pollutes stdout). Non-JSON stdout lines are skipped.
  Cursor's binary is `cursor-agent` (per `tools::binary`).
- An Agent/Thought entry grows while chunks keep the same voice and `messageId`
  (Cursor sends none) and nothing else comes in between.
- On cancel: open approvals are answered `cancelled` (spec MUST), unfinished tool
  calls are marked Failed (spec SHOULD, no Cancelled status in `ToolStatus`).
- Events are sent to the sink while the desk mutex is held, so their order is
  kept across threads. The sink must not call back into the driver synchronously.

## Verified live
- opencode 1.18.33 (`live_opencode_turn_with_an_approval`, `opencode/big-pickle`,
  `OPENCODE_CONFIG_CONTENT={"permission":{"bash":"ask"}}`): session id, model
  set through the config option, tool call Pending→Running→Done with output
  `hi\n`, approval with 3 choices answered "once", agent reply, Idle, Exited
  reported, no leftover process.
- Cursor 2026.10.01 (`live_cursor_turn`, after the CLI ended up logged in, see
  above): session id, thought chunks, tool call, approval ("Not in allowlist:
  echo", choices allow-once/allow-always/reject-once) answered, reply "hi",
  Idle, Exited. Before login, `session/new` returned -32000 with
  `data.message`, which the notice now shows.

## Not verified / gaps
- Model selection for Cursor (`session/set_model`) is unit-tested against its
  recorded `models` list but not exercised live. The hire picker currently
  offers only "Tool default" for opencode and Cursor, so `spawn.model` is None in practice.
- `spawn.effort` is not applied. opencode exposes no `thought_level` option, and
  Cursor encodes effort inside model ids.
- `spawn.launch.opts` are appended after `acp`, but neither tool currently gets any, so this is untested live.
- Cursor's blocking extensions `cursor/ask_question` and `cursor/create_plan` get
  -32601. How Cursor carries on after that has not been seen live.
- If a process exits while a grandchild keeps its stdout open, the exit is only
  noticed on `stop()`.
- Cursor left a detached `worker-server` daemon (its own design) running after
  the probes. It was not killed.
