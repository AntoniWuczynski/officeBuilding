import type { OfficeSnapshot, Project, Session, ToolKind, ToolOptions } from "../types";

/** A human's answer to a queued question: a chosen option or free text. */
export type QuestionAnswer =
  | { readonly kind: "option"; readonly optionId: string }
  | { readonly kind: "other"; readonly text: string };

/** Everything needed to start a new agent session. */
export interface HireRequest {
  readonly tool: ToolKind;
  readonly title: string;
  readonly model: string;
  readonly effort: string;
}

/** A hired agent that exited before it reached a desk. */
export interface HireFailure {
  /** The task it was hired for. */
  readonly title: string;
  readonly exitCode: number | null;
  /** The last non-blank lines it printed, as plain text. */
  readonly lastLines: readonly string[];
}

/** What a terminal panel hears from a session's in-app terminal. */
export interface TerminalHandlers {
  /** Output in order: the kept backlog first, then live chunks. */
  readonly output: (data: string) => void;
  /** The agent exited (at most once); `exitCode` is null when it is unknown. */
  readonly exit: (exitCode: number | null) => void;
  /** The terminal could not be reached. */
  readonly error: (message: string) => void;
}

/** Files dropped onto the window from Finder, at a point in CSS pixels. */
export interface FileDrop {
  readonly paths: readonly string[];
  readonly x: number;
  readonly y: number;
}

/**
 * The single seam between the UI and its data source. Phase 1 = `MockApi`
 * (in-browser, no native deps). Phase 2+ = `TauriApi` calling the Rust backend
 * (real discovery, PTY terminals, the MCP-exposed control surface). Swapping
 * one for the other is a single provider change — the UI depends only on this
 * interface.
 */
export interface DiscoveryApi {
  /** Current full snapshot of projects/sessions/questions/decisions/todos. */
  getSnapshot(): Promise<OfficeSnapshot>;

  /**
   * Subscribe to live snapshots (backend pushes on discovery/state changes).
   * Returns an unsubscribe function. The mock emits on every local mutation.
   */
  subscribe(listener: (snapshot: OfficeSnapshot) => void): () => void;

  /** Tools the app can start, each with its models and per-model efforts. */
  hireOptions(): Promise<readonly ToolOptions[]>;

  /** Add an employee: spawn a new agent session on a project. */
  spawnSession(projectId: string, request: HireRequest): Promise<Session>;

  /** Route a human answer back to the blocked session. */
  answerQuestion(questionId: string, answer: QuestionAnswer): Promise<void>;

  /** Tick a TODO off (by AI or human). */
  tickTodo(todoId: string): Promise<void>;

  /** Agent↔agent: post a message from one session to another. */
  sendAgentMessage(
    fromSessionId: string,
    toSessionId: string,
    text: string,
  ): Promise<void>;

  /**
   * Add a floor for a repo folder the human picked (its git root). A folder
   * that already has a floor resolves to that floor.
   */
  addFloor(path: string): Promise<Project>;

  /** Whether {@link pickFolder} can open a native folder picker here. */
  readonly canPickFolder: boolean;

  /** The platform's folder picker; null when the human cancels. */
  pickFolder(): Promise<string | null>;

  /**
   * Bring a discovered session's original terminal window to the front
   * (native macOS window activation). Only meaningful for `raise-window`.
   */
  focusSession(sessionId: string): Promise<void>;

  /**
   * Watch the in-app terminal of a `full`-control session: replays what it
   * kept, then streams. Returns a function that stops watching; the agent
   * keeps running.
   */
  subscribeTerminal(sessionId: string, handlers: TerminalHandlers): () => void;

  /**
   * Hear about hired agents that stopped before they ever reached a desk
   * (after the hire itself was reported as started). Returns an unsubscribe.
   */
  subscribeHireFailures(listener: (failure: HireFailure) => void): () => void;

  /**
   * Resume an ended session (from its floor's sign-out sheet) in an in-app
   * terminal, in the folder it ran in. Resolves once the agent has started.
   */
  resumeSession(sessionId: string): Promise<void>;

  /** Send keystrokes to a `full`-control session's terminal. */
  writeTerminal(sessionId: string, data: string): Promise<void>;

  /** Tell a `full`-control session's terminal its new size in cells. */
  resizeTerminal(sessionId: string, cols: number, rows: number): Promise<void>;

  /** Hear about files dropped onto the window. Returns an unsubscribe. */
  subscribeFileDrops(listener: (drop: FileDrop) => void): () => void;
}
