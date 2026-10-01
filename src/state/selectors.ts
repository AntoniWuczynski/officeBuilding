import type {
  Decision,
  FloorLight,
  OfficeSnapshot,
  Project,
  Question,
  Session,
  SessionState,
  Spend,
  TodoItem,
  ToolKind,
} from "../types";

/** Worst→best ordering; higher number = more urgent. */
export const SESSION_STATE_SEVERITY: Record<SessionState, number> = {
  error: 5,
  "waiting-human": 4,
  running: 3,
  thinking: 2,
  idle: 1,
  done: 0,
};

/** Human words for each state (never show the enum in the interface). */
export const STATE_LABEL: Record<SessionState, string> = {
  error: "Stuck",
  "waiting-human": "Needs you",
  running: "Working",
  thinking: "Thinking",
  idle: "Idle",
  done: "Finished",
};

export const TOOL_LABEL: Record<ToolKind, string> = {
  "claude-code": "Claude Code",
  codex: "Codex",
  opencode: "opencode",
  antigravity: "Antigravity",
  cursor: "Cursor",
  unknown: "Unknown tool",
};

/** Sessions that belong to a given project, in declared order. */
export function sessionsForProject(
  snapshot: OfficeSnapshot,
  projectId: string,
): Session[] {
  return snapshot.sessions.filter((s) => s.projectId === projectId);
}

/**
 * Floor light = worst state on the floor.
 *  red    = any session stuck on an error,
 *  yellow = any session waiting on a human,
 *  green  = any session working,
 *  off    = nobody working (an empty floor, or every agent idle).
 */
export function floorLight(sessions: readonly Session[]): FloorLight {
  let hasWaiting = false;
  let working = false;
  for (const s of sessions) {
    if (s.state === "error") return "red";
    if (s.state === "waiting-human") hasWaiting = true;
    if (isActive(s)) working = true;
  }
  if (hasWaiting) return "yellow";
  return working ? "green" : "off";
}

/** A session is "active" while it is working: running a tool or thinking. */
export function isActive(session: Session): boolean {
  return session.state === "running" || session.state === "thinking";
}

export interface Counts {
  readonly active: number;
  readonly total: number;
}

export function sessionCounts(sessions: readonly Session[]): Counts {
  return {
    active: sessions.filter(isActive).length,
    total: sessions.length,
  };
}

/** Project-level counts for the building exterior badges. */
export function projectCounts(
  snapshot: OfficeSnapshot,
  project: Project,
): Counts {
  return sessionCounts(sessionsForProject(snapshot, project.id));
}

/** Case-insensitive substring match over a project's name and path. */
export function matchesQuery(project: Project, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  return (
    project.name.toLowerCase().includes(q) ||
    project.path.toLowerCase().includes(q)
  );
}

export function searchProjects(
  projects: readonly Project[],
  query: string,
): Project[] {
  return projects.filter((p) => matchesQuery(p, query));
}

/** Case-insensitive match over a session's title, tool and state words. */
export function sessionMatches(session: Session, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  return (
    session.title.toLowerCase().includes(q) ||
    TOOL_LABEL[session.tool].toLowerCase().includes(q) ||
    STATE_LABEL[session.state].toLowerCase().includes(q)
  );
}

export function searchSessions(
  sessions: readonly Session[],
  query: string,
): Session[] {
  return sessions.filter((s) => sessionMatches(s, query));
}

function byUrgency(a: Question, b: Question): number {
  if (a.priority !== b.priority) return a.priority - b.priority;
  return a.askedAt.localeCompare(b.askedAt);
}

/** The whole building's question queue, most-urgent first. */
export function questionQueue(snapshot: OfficeSnapshot): Question[] {
  return [...snapshot.questions].sort(byUrgency);
}

/** One floor's question queue (its agents' questions and its decisions file), most-urgent first. */
export function questionsForProject(
  snapshot: OfficeSnapshot,
  projectId: string,
): Question[] {
  return snapshot.questions.filter((q) => q.projectId === projectId).sort(byUrgency);
}

/** Decisions recorded for a project, newest first. */
export function decisionsForProject(
  snapshot: OfficeSnapshot,
  projectId: string,
): Decision[] {
  return snapshot.decisions
    .filter((d) => d.projectId === projectId)
    .sort((a, b) => b.decidedAt.localeCompare(a.decidedAt));
}

/** A project's TODOs for one assignee, open items first. */
export function todosForProject(
  snapshot: OfficeSnapshot,
  projectId: string,
  assignee: TodoItem["assignee"],
): TodoItem[] {
  const items = snapshot.todos.filter(
    (t) => t.projectId === projectId && t.assignee === assignee,
  );
  return [...items.filter((t) => !t.done), ...items.filter((t) => t.done)];
}

export function openCount(items: readonly TodoItem[]): number {
  return items.filter((t) => !t.done).length;
}

/** Sum of spend across the given sessions. */
export function totalSpend(sessions: readonly Session[]): Spend {
  return sessions.reduce<Spend>(
    (acc, s) => ({
      usd: acc.usd + s.spend.usd,
      tokens: acc.tokens + s.spend.tokens,
      unpricedTokens: acc.unpricedTokens + s.spend.unpricedTokens,
    }),
    { usd: 0, tokens: 0, unpricedTokens: 0 },
  );
}

export function sessionById(
  snapshot: OfficeSnapshot,
  sessionId: string,
): Session | undefined {
  return snapshot.sessions.find((s) => s.id === sessionId);
}

/** "claude-opus-5-5, high effort", or null when the tool does not report its model. */
/** "3 helpers working", or null when the session has none. */
export function helpersLine(session: Session): string | null {
  const n = session.helpers.length;
  return n === 0 ? null : `${n} ${n === 1 ? "helper" : "helpers"} working`;
}

export function modelLine(session: Session): string | null {
  if (session.model === null) return null;
  return session.effort === null ? session.model : `${session.model}, ${session.effort} effort`;
}

export function formatUsd(usd: number): string {
  return `$${usd.toFixed(2)}`;
}

export function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M tokens`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}k tokens`;
  return `${tokens} tokens`;
}
