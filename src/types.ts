// Core data model for the office-building app. Mirrored in Rust (`src-tauri`)
// as `serde` structs so the Tauri command boundary is a 1:1 typed contract.
// Discriminated unions everywhere; no `any`, no loose strings.

/** Which agent tool a session belongs to. */
export type ToolKind =
  | "claude-code"
  | "codex"
  | "opencode"
  | "antigravity"
  | "cursor"
  | "unknown";

/**
 * Live state of a single agent session. Drives the employee animation and the
 * floor light.
 */
export type SessionState =
  | "error" //         crashed / failed — red
  | "waiting-human" // blocked on a human answer — hand-up
  | "running" //       actively executing a tool / long task
  | "thinking" //      model is generating
  | "idle" //          alive but nothing in flight
  | "done"; //         finished cleanly

/** How much control the app has over a session's terminal. */
export type ControlMode =
  | "full" //        app-spawned: interactive PTY inside the app
  | "raise-window" // discovered/hand-started: click raises its original terminal
  | "read-only"; //  fallback: transcript view only

/** Traffic-light for a floor (project), aggregated from its sessions. */
export type FloorLight = "green" | "yellow" | "red" | "off";

/** Dollars at API list prices. `unpricedTokens` (part of `tokens`) came from models with no public price. */
export interface Spend {
  readonly usd: number;
  readonly tokens: number;
  readonly unpricedTokens: number;
}

export interface QuestionOption {
  readonly id: string;
  readonly label: string;
}

/** A human-in-the-loop question surfaced by a blocked session. */
export interface Question {
  readonly id: string;
  readonly projectId: string;
  /** The asking session; empty for calls that come from a repo file. */
  readonly sessionId: string;
  readonly prompt: string;
  readonly options: readonly QuestionOption[];
  readonly allowOther: boolean;
  /**
   * Where the human answers it. "app": the app owns the session's terminal and
   * delivers the answer. "terminal": a hand-started session; the app can only
   * bring its own terminal window to the front. "file": an open call in the
   * repo's decisions file; the answer is written back there.
   */
  readonly answerVia: "app" | "terminal" | "file";
  /** Lower = more urgent; the queue is sorted ascending. */
  readonly priority: number;
  readonly askedAt: string;
  /** Background for the call (the file section's text); empty for agent questions. */
  readonly context: string;
  /** The repo file the call lives in, when it comes from one. */
  readonly sourceFile: string | null;
}

export interface Decision {
  readonly id: string;
  readonly projectId: string;
  readonly title: string;
  readonly detail: string;
  readonly decidedAt: string;
  readonly by: "ai" | "human";
}

export interface TodoItem {
  readonly id: string;
  readonly projectId: string;
  readonly text: string;
  readonly done: boolean;
  /** Who is expected to tick it off. */
  readonly assignee: "ai" | "human";
  /** The repo file the item lives in (TODO.md, FOUNDER_TODO.md); null for the app's own records. */
  readonly sourceFile: string | null;
}

/** A note one agent sent another (agent-to-agent messaging). */
export interface AgentMessage {
  readonly id: string;
  readonly fromSessionId: string;
  readonly toSessionId: string;
  readonly text: string;
  readonly sentAt: string;
}

export interface Session {
  readonly id: string;
  readonly tool: ToolKind;
  readonly projectId: string;
  readonly title: string;
  readonly state: SessionState;
  readonly control: ControlMode;
  /** Model id the session runs on, when the tool reports it. */
  readonly model: string | null;
  /** Reasoning effort id, when the tool reports it. */
  readonly effort: string | null;
  readonly lastActivityAt: string;
  readonly pendingQuestionIds: readonly string[];
  readonly spend: Spend;
  /** Sub-agents (plain or in a workflow) still working for this session. */
  readonly helpers: readonly Helper[];
}

/** A sub-agent at work for a session: it gets a tiny desk beside its session's. */
export interface Helper {
  readonly id: string;
  readonly state: SessionState;
}

export interface Project {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  readonly sessionIds: readonly string[];
}

/** One reasoning-effort level a model accepts. */
export interface EffortOption {
  readonly id: string;
  readonly label: string;
  readonly description: string;
}

/** A model a tool can run, with the efforts that model supports (they vary per model). */
export interface ModelOption {
  readonly id: string;
  readonly label: string;
  readonly efforts: readonly EffortOption[];
  readonly defaultEffort: string;
}

/** What the app can hire for one tool. Read from the tool's own config by the backend. */
export interface ToolOptions {
  readonly tool: ToolKind;
  readonly models: readonly ModelOption[];
  readonly defaultModel: string;
}

/** The full snapshot the UI renders from. Kept flat + normalised. */
export interface OfficeSnapshot {
  readonly projects: readonly Project[];
  readonly sessions: readonly Session[];
  readonly questions: readonly Question[];
  readonly decisions: readonly Decision[];
  readonly todos: readonly TodoItem[];
  readonly messages: readonly AgentMessage[];
}
