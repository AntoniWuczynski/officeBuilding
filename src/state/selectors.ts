import type {
  DeskHandover,
  DeskHandoverState,
  Connection,
  Decision,
  Editor,
  EditorList,
  EndedSession,
  FloorLight,
  FloorLink,
  ManagerCaps,
  ManagerFloor,
  ManagerLogEntry,
  NoteHold,
  NoteState,
  OfficeSnapshot,
  Project,
  QaFloor,
  QueuedNote,
  Question,
  Review,
  Session,
  SessionState,
  Spend,
  TodoItem,
  TodoSource,
  ToolKind,
  Verdict,
} from "../types";
import { relativeTime } from "../lib/time";

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

/** Sessions that ended recently on a project's floor, newest first. */
export function endedForProject(snapshot: OfficeSnapshot, projectId: string): EndedSession[] {
  return snapshot.ended.filter((e) => e.projectId === projectId);
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

/** What "Open in" opens for a desk or a helper, in words. */
export function checkoutLine(at: { readonly worktree: boolean; readonly branch: string | null }): string {
  if (!at.worktree) return "its floor's folder";
  return at.branch === null ? "its worktree" : `its worktree on branch ${at.branch}`;
}

/** "A, B and C". */
export function joinNames(names: readonly string[]): string {
  return names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** The name the backend gave `editor`, if the list knows it. */
export function editorLabel(list: EditorList | null, editor: Editor | null): string | undefined {
  return list?.editors.find((o) => o.editor === editor)?.label;
}

export interface EditorMenuItem {
  /** Null: let the backend choose. */
  readonly editor: Editor | null;
  /** The menu line: "Open in Cursor". */
  readonly label: string;
  /** The bare name, for sentences: "Cursor". */
  readonly name: string;
}

/** The menu's lines: the default first, then the other installed editors, in list order. */
export function editorMenuItems(list: EditorList | null): readonly EditorMenuItem[] {
  if (list === null) return [{ editor: null, label: "Open in editor", name: "your editor" }];
  const installed = list.editors.filter((o) => o.installed);
  const first = installed.find((o) => o.editor === list.defaultEditor);
  const ordered = first === undefined ? installed : [first, ...installed.filter((o) => o !== first)];
  return ordered.map((o) => ({ editor: o.editor, label: `Open in ${o.label}`, name: o.label }));
}

/** The floor's connection, if it has one (one connector exists today). */
export function connectionFor(snapshot: OfficeSnapshot, projectId: string): Connection | undefined {
  return snapshot.connections.find((c) => c.projectId === projectId);
}

/** The words on an item's badge: a file's name, "Linear · ENG-123", or null for the app's own. */
export function sourceBadge(source: TodoSource): string | null {
  switch (source.kind) {
    case "app":
      return null;
    case "file":
      return source.file;
    case "linear":
      return `Linear · ${source.identifier}`;
  }
}

/** The repo files the items come from, each once, in order. */
export function fileSources(items: readonly TodoItem[]): string[] {
  const files: string[] = [];
  for (const { source } of items) {
    if (source.kind === "file" && !files.includes(source.file)) files.push(source.file);
  }
  return files;
}

/** A link in words: "ENG · Engineering", then ", project Checkout" and ", label bug" when set. */
export function linkSummary(link: FloorLink): string {
  const project = link.project === null ? "" : `, project ${link.project.name}`;
  const label = link.label === null ? "" : `, label ${link.label}`;
  return `${link.teamKey} · ${link.teamName}${project}${label}`;
}

/** How a connection stands, as one sentence for a board. */
export function connectionLine(connection: Connection, now: Date): string {
  const { fetchedAt, error } = connection;
  if (connection.needsKey) return "Linear needs an API key before it can show this floor's issues.";
  if (fetchedAt === null) return error === null ? "Checking Linear…" : `Linear could not be checked: ${error}.`;
  const ago = relativeTime(fetchedAt, now);
  if (error !== null) return `Linear, as of ${ago}. The last check failed: ${error}.`;
  return `Linear, checked ${ago}.${connection.truncated ? " Only the first 200 issues are shown." : ""}`;
}

/** A floor's QA desk, if the floor has one (the setup floor has none). */
export function qaFor(snapshot: OfficeSnapshot, projectId: string): QaFloor | undefined {
  return snapshot.qa.find((q) => q.projectId === projectId);
}

export const plural = (n: number, one: string, many: string): string => `${n} ${n === 1 ? one : many}`;

/** What a QA desk is doing, as one sentence. */
export function qaLine(floor: QaFloor, now: Date): string {
  if (floor.run !== null) return `Reviewing #${floor.run.number} \u201c${floor.run.title}\u201d.`;
  const { status } = floor;
  switch (status.kind) {
    case "off":
      return "Off.";
    case "waiting":
      return "Asking GitHub\u2026";
    case "blocked":
      return `Cannot review: ${status.reason}.`;
    case "watching": {
      const open = status.open === 0 ? "No open pull requests of yours" : `${plural(status.open, "open pull request", "open pull requests")} of yours`;
      return `Watching ${status.repo}. ${open}, checked ${relativeTime(status.checkedAt, now)}.`;
    }
  }
}

/** How a review ended, in words. */
export function reviewLine(review: Review): string {
  const { outcome } = review;
  switch (outcome.kind) {
    case "posted":
      return outcome.findings === 0 ? "No issues found" : plural(outcome.findings, "finding", "findings");
    case "clean":
      return "Nothing to flag, nothing posted";
    case "failed":
      return `Not reviewed: ${outcome.reason}`;
  }
}

/** The stamp's lettering and how it is read out. */
export const STAMP_WORDS: Record<Verdict, { readonly label: string; readonly spoken: string }> = {
  pass: { label: "QA passed", spoken: "QA passed" },
  concerns: { label: "QA notes", spoken: "QA found problems" },
};

/** The floor's sessions split into those at ordinary desks and the QA reviewer, if it is seated. */
export function splitReviewer(
  sessions: readonly Session[],
  qa: QaFloor | undefined,
): { readonly seated: readonly Session[]; readonly reviewer: Session | undefined } {
  const id = qa?.enabled === true ? qa.run?.sessionId : null;
  const reviewer = id === null || id === undefined ? undefined : sessions.find((s) => s.id === id);
  return { seated: reviewer === undefined ? sessions : sessions.filter((s) => s !== reviewer), reviewer };
}

/** A floor's manager, if the floor has one (the setup floor has none). */
export function managerFor(snapshot: OfficeSnapshot, projectId: string): ManagerFloor | undefined {
  return snapshot.managers.find((m) => m.projectId === projectId);
}

/** The floor's sessions without its manager, and the manager's session when it has one. */
export function splitManager(
  sessions: readonly Session[],
  manager: ManagerFloor | undefined,
): { readonly rest: readonly Session[]; readonly managerSession: Session | undefined } {
  const id = manager?.status.kind === "hired" ? manager.status.sessionId : null;
  if (id === null) return { rest: sessions, managerSession: undefined };
  return { rest: sessions.filter((s) => s.id !== id), managerSession: sessions.find((s) => s.id === id) };
}

/** Why a note waits, to finish the sentence "Waiting: …". */
export const HOLD_WORDS: Record<NoteHold, string> = {
  busy: "it is busy",
  "waiting-on-you": "it is waiting on you",
  stuck: "it is stuck",
  finished: "it has finished",
  "you-typed": "you typed there in the last minute",
  "awaiting-turn": "it has not picked up the last note yet",
  "hand-started": "it was started outside the building, so nothing is typed into it",
  "no-idle-signal": "its tool does not tell the building when it is idle",
  paused: "the manager is paused",
};

export const NOTE_STATE_WORDS: Record<NoteState, string> = {
  queued: "Waiting",
  delivered: "Delivered",
  withdrawn: "Withdrawn by you",
  overridden: "Replaced by your message",
  dropped: "Dropped: the desk or the manager left",
  failed: "Could not be handed over",
};

/** The manager's desk light as a session state the desk's light already draws: green, yellow or off. */
export function managerLamp(manager: ManagerFloor): "running" | "waiting-human" | "done" {
  if (manager.status.kind === "absent" || manager.paused) return "done";
  return manager.escalated ? "waiting-human" : "running";
}

/** The manager in a few words: "Nobody is hired", "Starting", "Paused", or its session's state. */
export function managerLine(manager: ManagerFloor | undefined, session: Session | undefined): string {
  if (manager === undefined || manager.status.kind === "absent") return "Nobody is hired";
  if (manager.paused) return "Paused";
  return session === undefined ? "Starting" : STATE_LABEL[session.state];
}

/** "1 of 3 desks hired, $4.10 of $20.00 spent". */
export function capsLine(caps: ManagerCaps): string {
  return `${caps.hiredNow} of ${caps.maxDesks} desks hired, ${formatUsd(caps.spentUsd)} of ${formatUsd(caps.spendCeilingUsd)} spent`;
}

const REFUSED_WHAT: Partial<Record<string, string>> = { send_to_desk: "a note", hire_desk: "a hire", escalate: "a question for you" };

/** One log entry in words. */
export function logLine(entry: ManagerLogEntry): string {
  const { event } = entry;
  switch (event.kind) {
    case "note":
      return `To \u201c${event.toTitle}\u201d: ${event.text}`;
    case "hire":
      return `Hired ${TOOL_LABEL[event.tool]} for \u201c${event.task}\u201d`;
    case "escalation":
      return `Asked you: ${event.title}`;
    case "refused":
      return `Refused ${REFUSED_WHAT[event.tool] ?? "a request"}: ${event.reason}`;
  }
}

/** Why one of your own messages waits: the manager's reasons, with three of your own. */
export const QUEUE_WORDS: Record<NoteHold, string> = {
  ...HOLD_WORDS,
  "you-typed": "its terminal holds something you typed and have not sent",
  "awaiting-turn": "it has not picked up your last message yet",
  "no-idle-signal": "it has not told the building it is idle yet, which can take a minute after it starts",
};

/** Your own messages waiting for one desk, oldest first. */
export function queuedFor(snapshot: OfficeSnapshot, sessionId: string): readonly QueuedNote[] {
  return snapshot.queued.filter((n) => n.toSessionId === sessionId);
}

/** The handover entry of one desk's session, if it has one. */
export function handoverOf(desks: readonly DeskHandover[], sessionId: string): DeskHandover | undefined {
  return desks.find((d) => d.sessionId === sessionId);
}

/** What one desk's handover has to say, if anything. */
export function handoverFor(snapshot: OfficeSnapshot, sessionId: string): DeskHandoverState | undefined {
  return handoverOf(snapshot.handover.desks, sessionId)?.state;
}

/** The notes waiting for one desk. */
export function notesFor(manager: ManagerFloor | undefined, sessionId: string): readonly QueuedNote[] {
  return manager?.queue.filter((n) => n.toSessionId === sessionId) ?? [];
}
