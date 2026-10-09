import type { HarnessSetup, SetupItemKind, SetupView, Support } from "../types";

const yes: Support = { kind: "supported", note: null };
const no = (reason: string): Support => ({ kind: "unsupported", reason });
const unread = { skills: null, mcpServers: null, hooks: null, plugins: null, instructions: null } as const;
const all = (s: Support): Record<SetupItemKind, Support> => ({ skill: s, instructions: s, "mcp-server": s, hook: s });

const HARNESSES: readonly HarnessSetup[] = [
  {
    tool: "claude-code",
    installed: true,
    skills: [
      { name: "humanizer", description: "Remove signs of AI-generated writing from text." },
      { name: "todo-sync", description: "Reconcile TODO.md against the codebase." },
    ],
    mcpServers: ["brain"],
    hooks: [
      { id: "PreToolUse:0", event: "PreToolUse", matcher: "Bash", command: "guard.sh" },
      { id: "Stop:0", event: "Stop", matcher: null, command: "notify.sh" },
      { id: "Stop:1", event: "Stop", matcher: null, command: "sync.sh" },
    ],
    instructions: ["CLAUDE.md"],
    plugins: [
      { name: "superpowers@claude-plugins-official", enabled: true },
      { name: "vercel@claude-plugins-official", enabled: false },
    ],
    support: all(yes),
  },
  {
    tool: "codex",
    installed: true,
    skills: [{ name: "humanizer", description: "Remove signs of AI-generated writing from text." }],
    mcpServers: [],
    hooks: [{ id: "SessionStart:0", event: "SessionStart", matcher: null, command: "start.sh" }],
    instructions: ["AGENTS.md"],
    plugins: [{ name: "superpowers@openai-curated", enabled: true }],
    support: { ...all(yes), hook: { kind: "supported", note: "Codex skips a new hook until you trust it once with /hooks" } },
  },
  {
    tool: "antigravity",
    installed: true,
    skills: [],
    mcpServers: [],
    hooks: null,
    plugins: null,
    instructions: [],
    support: {
      skill: yes,
      instructions: yes,
      "mcp-server": no("Antigravity has no launch option for MCP servers"),
      hook: no("Antigravity has no launch option for hooks"),
    },
  },
  {
    tool: "opencode",
    installed: true,
    skills: [],
    mcpServers: [],
    hooks: [],
    plugins: [],
    instructions: [],
    support: { ...all(yes), hook: no("opencode's hooks are code plugins, not commands") },
  },
  { tool: "cursor", installed: true, ...unread, support: { ...all(no("Cursor's --plugin-dir carries skills only (checked 2026-10-08)")), skill: yes } },
];

/** Demo setup floor for the browser build. */
export function makeSetupView(): SetupView {
  return {
    items: [
      { kind: "skill", name: "release-notes", description: "Write release notes from the git log.", targets: { kind: "all" } },
      { kind: "skill", name: "floor-tour", description: "Walk a new agent through the repo.", targets: { kind: "only", tools: ["claude-code"] } },
      { kind: "instructions", name: "house-rules", description: "British English, no semicolons.", targets: { kind: "all" } },
      { kind: "mcp-server", name: "docs", description: "npx", targets: { kind: "all" } },
      { kind: "hook", name: "format", description: "PostToolUse (Edit|Write): ./format.sh", targets: { kind: "all" } },
    ],
    harnesses: HARNESSES,
    problems: [],
  };
}
