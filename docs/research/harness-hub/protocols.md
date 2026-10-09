<!-- Research notes, 2026-10-08. Sourced per line; UNVERIFIED marks claims not confirmed at a primary source. Synthesis: README.md. -->

# Section 1: ACP
- JSON-RPC 2.0; agents run as client subprocesses (https://agentclientprotocol.com/protocol/overview). Stdio transport: UNVERIFIED on that page (page does not state it).
- Stable protocol version 1; schema/v1 and schema/v2 dirs exist (https://github.com/agentclientprotocol/agent-client-protocol)
- Methods: initialize, authenticate, session/new, session/prompt, session/update (notif), session/request_permission, fs/read_text_file, fs/write_text_file, terminal/create|output|wait_for_exit|kill|release; optional session/load via loadSession capability (overview URL above)
- Governance: org agentclientprotocol, GOVERNANCE.md + MAINTAINERS.md, Apache-2.0 (repo URL above)
- SDKs: Rust `agent-client-protocol` (runtime) and `agent-client-protocol-schema` (types); TS `@agentclientprotocol/sdk`; Python, Kotlin, Java.
# Section 2 (partial): agent list https://agentclientprotocol.com/overview/agents
- Claude: adapter zed-industries/claude-agent-acp (Agent SDK based); Codex: adapter agentclientprotocol/codex-acp
- Listed without adapter label: Cursor (https://cursor.com/docs/cli/acp), Gemini CLI, OpenCode, Goose, Qwen Code, Kimi, Kiro, Copilot (preview), Augment, Cline, Factory Droid, Mistral Vibe, OpenHands, Junie
# Section 2: per harness
- Claude: ACP adapter @agentclientprotocol/claude-agent-acp (repo github.com/zed-industries/claude-agent-acp, listed under agentclientprotocol org; Apache-2.0; 2.6k stars; `preview` npm channel, no stable version stated); built on official Claude Agent SDK. Session resume docs: UNVERIFIED. Subscription auth: UNVERIFIED (see section 3).
- Codex: @agentclientprotocol/codex-acp (https://github.com/agentclientprotocol/codex-acp); stdio; spawns Codex App Server; auth ChatGPT login / CODEX_API_KEY / OPENAI_API_KEY; preview channel; session/load: UNVERIFIED.
- Gemini CLI: native, `gemini --acp` (https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/acp-mode.md). `--experimental-acp` not on page (UNVERIFIED); includes `unstable_setSessionModel`.
- opencode: native `opencode acp`, stdio; /undo /redo unsupported (https://opencode.ai/docs/acp/)
- Cursor CLI: native `agent acp`, stdio newline JSON-RPC, session/new + session/load, modes agent/plan/ask, cursor/* extension methods, auth cursor_login (https://cursor.com/docs/cli/acp). Team dashboard MCP unsupported.
- Antigravity: no official native ACP found. Zed registry lists it (https://zed.dev/acp/agent/antigravity-acp) ; community wrappers around `agy` (github.com/jameslunardi/agy-agent-acp relies on undocumented --output-format=stream-json; github.com/shindgew/agy-acp; github.com/shubzkothekar/antigravity-acp) with ToS-risk warnings. UNVERIFIED whether the registry binary is Google-published.
- Others listed in ACP agent list: Goose, Qwen Code, Kimi, Kiro, Copilot (preview), Augment, Cline, Factory Droid, Mistral Vibe, OpenHands, Junie.
# Section 3: alternatives
- Codex app-server (https://learn.chatgpt.com/docs/app-server): JSON-RPC 2.0 (no jsonrpc field) over stdio default (WS experimental); threads/turns/items; thread/start|resume|fork|list; turn/start|steer|interrupt; item/commandExecution/requestApproval, item/fileChange/requestApproval, etc; generate-ts / generate-json-schema; auth chatgpt managed OAuth, device code, apikey. Command itself marked experimental; many fields need experimentalApi.
- Claude: `claude -p --output-format stream-json` (+--verbose, --include-partial-messages); events system/init, api_retry, permission_denied; --resume <id>; --permission-prompt-tool; (https://code.claude.com/docs/en/headless). `--bare` ignores OAuth/keychain (API key only).
  POLICY: "Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK" (https://code.claude.com/docs/en/agent-sdk/overview). Help article: https://support.claude.com/en/articles/13189465-log-in-to-your-claude-account
- Gemini headless: --output-format json | stream-json events init/message/tool_use/tool_result/error/result (https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/headless.md). Auth: not covered on page (UNVERIFIED).
# Section 4: TUI / resume
- ACP/SDK has no TUI by itself (protocol is headless). Claude: transcripts at ~/.claude/projects/<project>/<id>.jsonl; `-p`/SDK sessions are hidden from picker and --continue but `claude --resume <session-id>` works from any dir (https://code.claude.com/docs/en/sessions). Do not resume same session in two processes at once (interleaved transcript). Whether claude-agent-acp sessions use same store/ID: UNVERIFIED (SDK says it runs Claude Code binary).
- Codex: app-server thread/resume; cross-resume with `codex resume`: UNVERIFIED.
- Cursor: session/load supported; interop with TUI UNVERIFIED.
