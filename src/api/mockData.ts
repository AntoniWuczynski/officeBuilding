import type { Helper, OfficeSnapshot } from "../types";

/** `count` demo helpers (sub-agents) for one session, a few of them thinking. */
function helpers(sessionId: string, count: number): Helper[] {
  return Array.from({ length: count }, (_, i) => ({ id: `${sessionId}-h${i}`, state: i % 7 === 3 ? "thinking" : "running" }));
}

/**
 * Deterministic seed snapshot for Phase 1 (demo data, not real sessions). A few
 * projects (floors) each with a handful of agent sessions (desks) across
 * different tools and states, plus per-project questions, decisions and TODOs
 * for the manager's office.
 */
export function makeSeedSnapshot(): OfficeSnapshot {
  return {
    projects: [
      { id: "p-shop", name: "corner_shop", path: "~/code/corner_shop", sessionIds: ["s-1", "s-2", "s-3"] },
      { id: "p-office", name: "officeBuilding", path: "~/code/officeBuilding", sessionIds: ["s-4", "s-5"] },
      { id: "p-arcade", name: "arcade", path: "~/code/arcade", sessionIds: ["s-6"] },
      { id: "p-notes", name: "notes", path: "~/code/notes", sessionIds: [] },
    ],
    sessions: [
      { id: "s-1", tool: "claude-code", projectId: "p-shop", title: "audit checkout", state: "running", control: "full", model: "claude-opus-5-5", effort: "high", lastActivityAt: "2026-09-28T15:40:00Z", pendingQuestionIds: [], spend: { usd: 1.82, tokens: 412_000, unpricedTokens: 0 }, helpers: helpers("s-1", 3) },
      { id: "s-2", tool: "codex", projectId: "p-shop", title: "orders ingest fix", state: "waiting-human", control: "full", model: "gpt-6-astra", effort: "xhigh", lastActivityAt: "2026-09-28T15:38:00Z", pendingQuestionIds: ["q-1"], spend: { usd: 0.44, tokens: 96_000, unpricedTokens: 0 }, helpers: [] },
      { id: "s-3", tool: "opencode", projectId: "p-shop", title: "map-tiles mirror", state: "error", control: "raise-window", model: null, effort: null, lastActivityAt: "2026-09-28T15:20:00Z", pendingQuestionIds: ["q-2"], spend: { usd: 0.12, tokens: 22_000, unpricedTokens: 0 }, helpers: [] },
      { id: "s-4", tool: "claude-code", projectId: "p-office", title: "isometric floors", state: "thinking", control: "full", model: "claude-fable-5-1", effort: "max", lastActivityAt: "2026-09-28T15:41:00Z", pendingQuestionIds: [], spend: { usd: 2.05, tokens: 508_000, unpricedTokens: 0 }, helpers: helpers("s-4", 250) },
      { id: "s-5", tool: "antigravity", projectId: "p-office", title: "character sprites", state: "idle", control: "read-only", model: null, effort: null, lastActivityAt: "2026-09-28T14:55:00Z", pendingQuestionIds: [], spend: { usd: 0.0, tokens: 0, unpricedTokens: 0 }, helpers: [] },
      { id: "s-6", tool: "codex", projectId: "p-arcade", title: "netcode prototype", state: "idle", control: "full", model: "gpt-5.6-terra", effort: "medium", lastActivityAt: "2026-09-28T13:10:00Z", pendingQuestionIds: [], spend: { usd: 3.7, tokens: 910_000, unpricedTokens: 0 }, helpers: [] },
    ],
    ended: [
      { id: "e-1", tool: "claude-code", projectId: "p-shop", title: "refund emails", endedAt: "2026-09-28T15:10:00Z", model: "claude-sonnet-5-5", effort: "medium" },
      { id: "e-2", tool: "codex", projectId: "p-shop", title: "price import", endedAt: "2026-09-28T12:00:00Z", model: "gpt-6-astra", effort: "high" },
    ],
    questions: [
      { id: "q-2", projectId: "p-shop", sessionId: "s-3", prompt: "The tile mirror keeps returning 429. Switch to the paid endpoint or back off?", options: [ { id: "o-1", label: "Use the paid endpoint (TILE_ENDPOINTS)" }, { id: "o-2", label: "Exponential back-off, keep the free mirrors" } ], allowOther: true, answerVia: "terminal", priority: 0, askedAt: "2026-09-28T15:21:00Z", context: "", sourceFile: null },
      { id: "q-1", projectId: "p-shop", sessionId: "s-2", prompt: "Persist EventType onto entries (schema bump) or keep it prefill-only?", options: [ { id: "o-3", label: "Schema bump (Zod, Pydantic and parity)" }, { id: "o-4", label: "Prefill-only, no persist" } ], allowOther: true, answerVia: "app", priority: 1, askedAt: "2026-09-28T15:39:00Z", context: "", sourceFile: null },
      { id: "q-3", projectId: "p-office", sessionId: "", prompt: "OB-001: Should the TODO panels read each repo's TODO.md?", options: [ { id: "o-5", label: "Read TODO.md, FOUNDER_TODO.md and FOUNDER_DECISIONS.md" }, { id: "o-6", label: "Keep the app's own list" } ], allowOther: true, answerVia: "file", priority: 0, askedAt: "2026-09-29T19:52:00Z", context: "Repos already keep TODO.md, and the notes repo splits the human's work into FOUNDER_TODO.md and FOUNDER_DECISIONS.md. Reading them keeps one source of truth.", sourceFile: "FOUNDER_DECISIONS.md" },
    ],
    decisions: [
      { id: "d-1", projectId: "p-office", title: "Backend based on CCC", detail: "Reuse claude-command-center for discovery and spawning. Our office UI and manager sidebar sit on top.", decidedAt: "2026-09-28T16:00:00Z", by: "human" },
      { id: "d-2", projectId: "p-office", title: "2.5D cutaway building", detail: "CSS 3D doll's-house view, not true 3D, for v1.", decidedAt: "2026-09-29T15:00:00Z", by: "human" },
      { id: "d-3", projectId: "p-shop", title: "Checkout audit runs weekly", detail: "Scheduled every Monday; findings filed to TODO.md.", decidedAt: "2026-09-27T09:00:00Z", by: "ai" },
    ],
    todos: [
      { id: "t-1", projectId: "p-office", text: "Evaluate the CCC HTTP API on the Mac", done: false, assignee: "human", sourceFile: null },
      { id: "t-2", projectId: "p-office", text: "Generate the app icons", done: false, assignee: "human", sourceFile: null },
      { id: "t-3", projectId: "p-office", text: "Scaffold the mock UI", done: true, assignee: "ai", sourceFile: null },
      { id: "t-4", projectId: "p-office", text: "Wire tauri-plugin-pty for spawned sessions", done: false, assignee: "ai", sourceFile: null },
      { id: "t-5", projectId: "p-shop", text: "Rotate the tile API key", done: false, assignee: "human", sourceFile: null },
      { id: "t-6", projectId: "p-shop", text: "Refresh map-tiles visual baselines", done: true, assignee: "ai", sourceFile: null },
      { id: "t-7", projectId: "p-shop", text: "Fix the orders ingest dedupe", done: false, assignee: "ai", sourceFile: null },
    ],
    messages: [],
  };
}
