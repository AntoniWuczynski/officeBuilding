# Codex structured driver: notes

Installed Codex is **0.161.0** (the brief said 0.154.0). Sources used: `codex app-server generate-ts` and `generate-json-schema` (plus `generate-ts --experimental`), the protocol page at https://learn.chatgpt.com/docs/app-server, and the live runs below.

## Layout

- `codex.rs`: spawns the process, runs the reader and stderr threads, implements `Driver`.
- `codex/mapper.rs`: a pure state machine. Lines go in, `Out::Event` / `Out::Send` come out.
- `codex/ask.rs`: turns approval requests into title, detail and choices.
- `codex/wire.rs`: typed protocol structs.
- `codex/tests.rs`: replays of recorded lines, plus three `#[ignore]`d live tests.

## Protocol used

- The wire format is JSON-RPC with no `"jsonrpc"` field, one message per line.
- Spawn: `<login shell> -l -c "exec [env K=V …] codex app-server -c check_for_update_on_startup=false <launch opts>"` in `spawn.cwd`. It goes through the login shell, like the terminal desks, so `codex` is found on the user's PATH. `exec` keeps the child pid as Codex's own.
- Handshake: `initialize` { clientInfo, capabilities { experimentalApi: false, requestAttestation: false } } → `initialized` notification → `thread/start` { cwd, model? }.
- `thread.id` from the `thread/start` result becomes `Event::SessionId`. It names the rollout file: `~/.codex/sessions/YYYY/MM/DD/rollout-…-<thread.id>.jsonl` (verified, and `thread.sessionId` was equal to it).
- `turn/start` { threadId, input: [{type: text, text, text_elements: []}], clientUserMessageId, effort? }. A prompt sent while a turn runs goes as `turn/steer` { expectedTurnId }. A steer that fails because the turn just ended is re-sent as a new turn.
- `turn/interrupt` { threadId, turnId }. The turn then completes with `status: interrupted`.
- Notifications mapped:
  - `turn/started` and `turn/completed` (completed or interrupted → Idle, failed → Error)
  - `item/started` and `item/completed`: userMessage, agentMessage, reasoning, commandExecution, fileChange, mcpToolCall, dynamicToolCall, webSearch, plan, contextCompaction
  - `item/agentMessage/delta`
  - `item/reasoning/summaryPartAdded` and `summaryTextDelta`
  - `item/commandExecution/outputDelta`
  - `item/fileChange/patchUpdated`
  - `item/mcpToolCall/progress`
  - `turn/plan/updated` (as a Notice)
  - `serverRequest/resolved` (an approval withdrawn)
  - `error`, `warning`
  - Everything else is ignored.
- A userMessage item carries our `clientUserMessageId` as `clientId`, so it replaces the User entry the driver emitted itself.
- Server requests mapped to `Event::Approval`:
  - `item/commandExecution/requestApproval`: choices come from `availableDecisions`, which the schema marks experimental but 0.161.0 sends anyway. If it is absent, the defaults are accept, acceptForSession, the proposed execpolicy and network amendments, decline and cancel.
  - `item/fileChange/requestApproval`: accept, acceptForSession, decline, cancel. The diff comes from the item.
  - `item/permissions/requestApproval`: grant for the turn, grant for the session (echoing the requested profile), or decline (empty grant).
  - `item/tool/requestUserInput`: only a single question with options. Anything else gets a JSON-RPC error.
  - `mcpServer/elicitation/request`: a URL, or a form with no fields, can be accepted. A form that has fields can only be declined or cancelled.
  - Legacy v1 `execCommandApproval` and `applyPatchApproval` (ReviewDecision).
  - `item/tool/call`, `account/chatgptAuthTokens/refresh`, `attestation/generate` and any unknown method get error -32601. Unparseable params get -32602 plus a Notice.
- Choice ids are indices ("0", "1", …). The approval id is the JSON-RPC request id.

## Verified live (Codex 0.161.0, ChatGPT Plus login)

1. A raw protocol probe (Python, now deleted) ran "Reply with the word ok". It confirmed the handshake, the message shapes, `thread.id` matching the rollout file name, and `effort: "low"` being accepted on `turn/start`.
2. A raw probe with `approvalPolicy: untrusted` and a read-only sandbox, running `touch probe_ok.txt`. The recorded request:
   `{"method":"item/commandExecution/requestApproval","id":0,"params":{"kind":"command",…,"command":"/bin/zsh -lc 'touch probe_ok.txt'","commandActions":[{"type":"unknown","command":"touch probe_ok.txt"}],"proposedExecpolicyAmendment":["touch","probe_ok.txt"],"availableDecisions":["accept",{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["touch","probe_ok.txt"]}},"cancel"]}}`
   Answering `{"id":0,"result":{"decision":"accept"}}` created the file, and `serverRequest/resolved` followed. **Note:** the decision list had no decline or acceptForSession. Codex chooses what it offers.
3. `live_turn` (through the driver): SessionId arrived, the reply was "ok", a rollout file named with the id exists, and `stop` gave `Exited { message: None }`.
4. `live_approval` (through the driver, `Launch.env` CODEX_HOME = temp home with auth.json linked, `approval_policy = "on-request"`, `sandbox_mode = "read-only"`): the approval showed, the tool went Pending, the state went WaitingHuman, then `answer` sent the reply and Resolved, Running, Done and Idle followed. The file was created.
5. `live_cancel`: cancelling on the first agent delta gave a "Stopped." notice and Idle.
6. Side finding: `approval_policy = "untrusted"` in config.toml makes 0.161.0 exit with "no longer supported" (the protocol field still accepts it). The driver reported this as `Exited { code: Some(1), message: Some("Codex stopped: Error: approval_policy = \"untrusted\" is no longer supported; remove this setting") }`. That exercised the stderr tail.

Run the live tests with `cargo test --lib live_ -- --ignored --nocapture`.

## Not verified live

- `turn/steer`, file-change, permissions, user-input and elicitation approvals. These are mapped from the schema and unit-tested only.
- `thread/start` with a non-default `model`.
- Launch `opts` (`-c …` args). `setup::inject::Opt` is not nameable outside `setup`, so no test builds one. The args are joined by the existing `Launch::args`.

## Behaviour to be aware of

- The driver does not set an approval policy or sandbox. The user's Codex config decides, as it does for terminal desks.
- The sink is called while the driver's internal lock is held, to keep events in order. A sink must not call `Driver` methods synchronously from inside the callback.
