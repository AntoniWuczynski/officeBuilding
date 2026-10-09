<!-- Research notes, 2026-10-08. Sourced per line; UNVERIFIED marks claims not confirmed at a primary source. Synthesis: README.md. -->

# Section 1: Landscape (GitHub API stats pulled 2026-10-08 via `gh api repos/<r>`)
Hosts / multiplexers
- Zed External Agents (ACP): Claude, Codex, OpenCode, Copilot, Cursor, Pi, Gemini CLI via ACP Registry; each agent owns auth; Zed Skills do NOT apply to external agents; Zed MCP servers may be forwarded. https://zed.dev/docs/ai/external-agents . Zed 91k stars, active.
- ACP: Apache-2.0, 4.4k stars, pushed 2026-10-08 https://github.com/agentclientprotocol/agent-client-protocol ; registry launched 2026-01-28 by JetBrains+Zed https://blog.jetbrains.com/ai/2026/01/acp-agent-registry/ (agent counts from secondary blogs: UNVERIFIED)
- Toad (AGPL-3.0, 3.5k, last push 2026-05-26): TUI front end over ACP https://github.com/batrachianai/toad
- Conductor (Melty Labs, Mac): Claude Code + Codex in worktrees; closed source/licence UNVERIFIED https://www.conductor.build
- Crystal (MIT, 3.1k, last push 2026-02-26, deprecated Feb 2026) -> Nimbalyst (MIT, 1.9k, active): Claude Code + Codex, OpenCode/Copilot alpha https://nimbalyst.com/crystal https://github.com/Nimbalyst/nimbalyst
- Claude Squad (AGPL-3.0, 8.6k, last push 2026-08-20): tmux+worktrees; Claude/Codex/Gemini/Aider https://github.com/smtg-ai/claude-squad
- vibe-kanban (Apache-2.0, 28k, pushed 2026-09-19): company bloop shut down 2026-04-10, community-maintained https://vibekanban.com/shutdown
- Emdash (Apache-2.0, 5.9k, active): 20+ providers, worktrees, YC W26 https://emdash.sh/ https://github.com/generalaction/emdash
- Sculptor (Imbue): Claude Code+Codex in Docker containers, beta; licence UNVERIFIED https://hn.svelte.dev/item/45427697
- Warp: third-party CLI agents (Claude, Codex, OpenCode notifications via plugin; Gemini etc. no notifications) https://docs.warp.dev/agents/cli-agents/overview/ . Hooks/skills unification: none.
- Cursor / JetBrains: Cursor is available inside JetBrains via ACP https://blog.jetbrains.com/ai/tag/acp/ ; Junie ACP status UNVERIFIED.
- opcode/Claudia (AGPL-3.0, 22k, pushed 2026-09-18): Claude Code only GUI https://github.com/winfunc/opcode
Config sync
- rulesync (MIT, 1.5k, pushed today): ~70 targets; rules, MCP, commands, subagents, skills, hooks, permissions https://github.com/dyoshikawa/rulesync
- ruler (MIT, 2.9k, pushed 2026-09-30): 32 agents; rules, MCP, experimental skills/subagents; no hooks listed https://github.com/intellectronica/ruler
- openskills (10.8k, licence NOASSERTION, last push 2026-01-18): skills via AGENTS.md only https://github.com/numman-ali/openskills ; skillport (MIT, 414, 2026-06-24): skills CLI+MCP https://github.com/gotalab/skillport
- MCPM (MIT, 1.0k, active): MCP servers/profiles across clients https://github.com/pathintegral-institute/mcpm.sh ; mcp-router: ARCHIVED, NOASSERTION/Sustainable Use licence https://github.com/mcp-router/mcp-router
Gap: hosts (Zed/Conductor/Emdash/Toad) unify the *session UI* and leave skills/hooks/MCP to each CLI; sync tools (rulesync) unify *files* but are headless CLIs with no live UI, no per-harness verification, no agent-driven editing. Nobody found combining live multi-harness terminals + one managed config with per-item targeting + drift/health view. Closest: rulesync (config) + Emdash (hosting) - UNVERIFIED that none integrate.

# Section 2: Terms risk
Anthropic (primary): https://code.claude.com/docs/en/legal-and-compliance
- OAuth is "intended exclusively" for subscribers' ordinary use of Claude Code and native Anthropic apps; third parties may not offer Claude.ai login or "route requests through Free, Pro, or Max plan credentials on behalf of their users"; may not "collect, store, or intermediate Claude.ai credentials or session tokens".
- Explicitly allowed: "an end user [signing] in to the unmodified Claude Code binary with their own Claude subscription", including on a platform that hosts it. Binary must be unmodified; no resale.
- Agent SDK: no claude.ai login/rate limits for third-party products unless approved; branding: may not use "Claude Code" / mimic its visuals https://code.claude.com/docs/en/agent-sdk/overview
- 2026-04-04: subscription limits stopped covering third-party harnesses (OpenClaw), billed as extra usage (secondary sources) https://lilting.ch/en/articles/anthropic-claude-code-openclaw-third-party-paygo
=> Launching the user's own official `claude` in a PTY = explicitly OK. Reading/reusing the OAuth token or SDK-with-subscription = prohibited. Don't brand the app/characters "Claude Code".
OpenAI: Codex supports ChatGPT sign-in or API key; creds cached in ~/.codex/auth.json "treat like a password" https://learn.chatgpt.com/docs/auth . OpenAI documents an open-source-partner OAuth path (https://developers.openai.com/siwc/quickstart per search; not fetched). Token reuse by third parties via proxy is unofficial (https://developer.puter.com/tutorials/openai-oauth/). Launching the official codex CLI: no restriction found. No explicit ToS clause fetched: UNVERIFIED.
Google: forum reports of mass 403 bans for using Gemini CLI/Antigravity OAuth in third-party tools (opencode, OpenClaw); staff quoted ToS "products Google does not provide" https://discuss.ai.google.dev/t/urgent-mass-403-tos-bans-on-gemini-api-antigravity-for-open-source-cli-users-paid-tier/124508 . Google primary terms text not fetched: UNVERIFIED. Launching official CLIs unmodified is low risk; do not touch tokens.

# Section 3: Churn (GitHub API + changelog, 2026-10-08)
- Claude Code: 100 stable releases between 2026-06-12 and 2026-10-08 (~0.85/day); CHANGELOG has 416 versions, latest 2.1.294. In the newest 60 versions, lines mentioning: settings 210, plugin 269, mcp 201, hook 134, skill 101, deprecat/removed 38 (keyword counts, not all breaking). Example: "Removed the deprecated TaskOutput tool ... `taskOutputMaxChars` setting ... no longer have any effect". https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md
- Codex: 60 releases (all, incl. prerelease) 2026-09-26 to 2026-10-08; 12 stable in 15 days (2026-09-22..10-07). https://github.com/openai/codex/releases
- opencode: 96 stable releases 2026-04-27 to 2026-10-06. https://github.com/anomalyco/opencode/releases
- Gemini CLI: 13 stable releases 2026-07-28 to 2026-10-06 (plus nightlies). https://github.com/google-gemini/gemini-cli/releases
Implication: daily-cadence upstreams; design adapters as small per-harness files with a version-probe and golden-file tests, avoid modelling every setting. Config-format-specific breaking-change rate not quantified: UNVERIFIED.
