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
  | "read-only" //   fallback: transcript view only
  | "structured"; // app-driven over the tool's protocol: opens a conversation panel

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
  /** What it left running, each beside its desk: shells as seashells, everything else as a pet. */
  readonly pets: readonly Pet[];
  /** It works in a linked git worktree of its floor's repo: a plant sits on its desk. */
  readonly worktree: boolean;
}

/** A shell running a background or long command, Claude Code's `Monitor` watching one, a browser the agent drives, or any other program it started. */
export type PetKind = "shell" | "monitor" | "browser" | "process";

export interface Pet {
  readonly id: string;
  readonly kind: PetKind;
  /** The command, or a monitor's description, shortened. */
  readonly label: string;
}

/** A session whose agent has left: listed on its floor's sign-out sheet to resume. */
export interface EndedSession {
  readonly id: string;
  readonly tool: ToolKind;
  readonly projectId: string;
  readonly title: string;
  /** Its last activity. */
  readonly endedAt: string;
  readonly model: string | null;
  readonly effort: string | null;
}

/** A sub-agent at work for a session: it gets a tiny desk beside its session's. */
export interface Helper {
  readonly id: string;
  readonly state: SessionState;
  /** It works in a linked git worktree: a plant sits on its tiny desk. */
  readonly worktree: boolean;
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
  /** Installed on this Mac. An unavailable tool is listed greyed out, with no models. */
  readonly available: boolean;
  readonly models: readonly ModelOption[];
  readonly defaultModel: string;
  /** Null when the tool has no ultracode; otherwise its default (Claude Code). */
  readonly ultracode: boolean | null;
  /** It can run as a structured desk: no terminal, typed messages and approvals. */
  readonly structured: boolean;
}

export type ToolStatus = "pending" | "running" | "done" | "failed";

/** One line of a structured desk's transcript; a later entry with the same id replaces it. */
export type ConversationEntry =
  | { readonly kind: "user"; readonly id: string; readonly text: string }
  | { readonly kind: "agent"; readonly id: string; readonly text: string }
  | { readonly kind: "thought"; readonly id: string; readonly text: string }
  | { readonly kind: "tool"; readonly id: string; readonly title: string; readonly status: ToolStatus; readonly detail: string }
  | { readonly kind: "notice"; readonly id: string; readonly text: string };

/** What a field of an approval takes. */
export type FieldKind =
  | { readonly kind: "text" }
  | { readonly kind: "secret" }
  | { readonly kind: "number" }
  | { readonly kind: "boolean" }
  | { readonly kind: "pick"; readonly options: readonly string[]; readonly other: boolean };

/** Something the human fills in before answering: a question's answer, a form field. */
export interface ApprovalField {
  readonly id: string;
  readonly label: string;
  readonly kind: FieldKind;
  readonly required: boolean;
}

/** The agent asks before doing something, or asks a question; a choice that lets it go ahead sends the fields' values. */
export interface Approval {
  readonly id: string;
  readonly title: string;
  readonly detail: string;
  readonly fields: readonly ApprovalField[];
  readonly choices: readonly { readonly id: string; readonly label: string; readonly allow: boolean }[];
}

/** A structured desk as its conversation panel opens on it. */
export interface ConversationBacklog {
  readonly transcript: readonly ConversationEntry[];
  readonly state: SessionState;
  readonly approval: Approval | null;
  /** Why the agent is gone, once it is. */
  readonly ended: string | null;
}

/** A change to a structured desk, pushed while its panel is open. */
export interface ConversationUpdate {
  readonly sessionId: string;
  readonly entry: ConversationEntry | null;
  readonly state: SessionState;
  readonly approval: Approval | null;
  readonly ended: string | null;
}

/** One usage-limit window of a tool, as the tool itself last measured it. */
export interface UsageWindow {
  readonly id: string;
  /** e.g. "5-hour session", "Week", "Week · Fable". */
  readonly label: string;
  /** Share of the window used, 0–100. */
  readonly percent: number;
  /** When the window resets (ISO 8601), if the tool says. */
  readonly resetsAt: string | null;
}

/** The usage limits of one installed tool, shown on the signs beside the building. */
export interface ToolUsage {
  readonly tool: ToolKind;
  /** When the tool last measured these numbers (ISO 8601): the "as of" time. */
  readonly checkedAt: string | null;
  readonly windows: readonly UsageWindow[];
  /** Why there are no numbers, when there are none. */
  readonly unavailable: string | null;
}

/** The full snapshot the UI renders from. Kept flat + normalised. */
export interface OfficeSnapshot {
  readonly projects: readonly Project[];
  readonly sessions: readonly Session[];
  /** Sessions that ended recently, newest first: each floor's sign-out sheet. */
  readonly ended: readonly EndedSession[];
  readonly questions: readonly Question[];
  readonly decisions: readonly Decision[];
  readonly todos: readonly TodoItem[];
  readonly messages: readonly AgentMessage[];
  /** The setup floor's project id (it may not be standing). */
  readonly setupProjectId: string;
}

/** Which harnesses a setup item applies to. */
export type Targets = { readonly kind: "all" } | { readonly kind: "only"; readonly tools: readonly ToolKind[] };

export type SetupItemKind = "skill" | "instructions" | "mcp-server" | "hook";

/** Whether a harness can take an item kind from the building at launch. */
/** Whether a harness can take an item kind from the building at launch; `note` says what the human still has to do. */
export type Support = { readonly kind: "supported"; readonly note: string | null } | { readonly kind: "unsupported"; readonly reason: string };

export interface SkillInfo {
  readonly name: string;
  readonly description: string;
}

/** One of a tool's own command hooks; `id` (`<event>:<n>`) names it for copying. */
export interface HookInfo {
  readonly id: string;
  readonly event: string;
  readonly matcher: string | null;
  readonly command: string;
}

export interface PluginInfo {
  readonly name: string;
  readonly enabled: boolean;
}

/** What one harness already has at user level. A list is null when it is not read yet. */
export interface HarnessSetup {
  readonly tool: ToolKind;
  readonly installed: boolean;
  readonly skills: readonly SkillInfo[] | null;
  readonly mcpServers: readonly string[] | null;
  readonly hooks: readonly HookInfo[] | null;
  readonly plugins: readonly PluginInfo[] | null;
  /** The tool's user-level instructions file, when it has one; null when the app does not know where it is. */
  readonly instructions: readonly string[] | null;
  readonly support: Readonly<Record<SetupItemKind, Support>>;
}

/** One of the building's setup items. */
export interface SetupItem {
  readonly kind: SetupItemKind;
  readonly name: string;
  /** A skill's description, the first line of instructions, a server's command, a hook's event and command. */
  readonly description: string;
  readonly targets: Targets;
}

/** The setup floor: the building's own items and each harness's inventory. */
export interface SetupView {
  readonly items: readonly SetupItem[];
  readonly harnesses: readonly HarnessSetup[];
  /** What in the library could not be used, in words. */
  readonly problems: readonly string[];
}
