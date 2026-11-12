import type { ConnectorKind, DeskView, ConversationBacklog, Editor, FloorLink, LinearKeyStatus, LinearTeam, EditorList, Place, ConversationUpdate, OfficeSnapshot, Project, Session, Settings, SettingsPatch, SetupItemKind, SetupView, Shell, Targets, ToolKind, ToolOptions, ToolUsage } from "../types";

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
  /** Run with ultracode on (Claude Code only, a setting beside the effort). */
  readonly ultracode: boolean;
  /** The view its desk opens in. Chat without a terminal starts a structured desk. */
  readonly view: DeskView;
}

/** A hired agent that exited before it reached a desk. */
export interface HireFailure {
  /** The task it was hired for. */
  readonly title: string;
  readonly exitCode: number | null;
  /** The last non-blank lines it printed, as plain text. */
  readonly lastLines: readonly string[];
}

/** A terminal's size in cells. */
export interface TerminalSize {
  readonly cols: number;
  readonly rows: number;
}

/** What a terminal panel hears from a session's in-app terminal. */
export interface TerminalHandlers {
  /**
   * Output in order: the kept backlog first, with the terminal size it was
   * drawn at, then live chunks without one.
   */
  readonly output: (data: string, drawnAt?: TerminalSize) => void;
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

  /** Usage limits of each installed tool, re-read from the tools' own files on every call. `refresh` has Claude Code check its usage first. */
  toolUsage(refresh: boolean): Promise<readonly ToolUsage[]>;

  /** The setup floor's items and each harness's own setup, re-read on every call. */
  setupView(): Promise<SetupView>;

  /** The app's settings, read from the backend's settings file. */
  getSettings(): Promise<Settings>;

  /** Change settings: each group in `patch` replaces that group. Resolves to the saved settings. */
  updateSettings(patch: SettingsPatch): Promise<Settings>;

  /** The editors the building knows, which are installed, and the default. Looks afresh on every call. */
  editors(): Promise<EditorList>;

  /**
   * Open the folder behind a desk, a helper's tiny desk or a floor in an editor
   * (null: the default). Resolves to the editor used. Rejects when the place is
   * unknown, its folder is gone, or no editor could be started.
   */
  openInEditor(place: Place, editor: Editor | null): Promise<Editor>;

  /** Tell the backend which desk's panel is open (null: none), so it does not alert for it. */
  setOpenDesk(sessionId: string | null): Promise<void>;

  /** Send a test alert in three seconds, by the methods switched on. */
  testAlert(): Promise<void>;

  /** Hear when anything the Setup panel shows may have changed on disk. Returns an unsubscribe. */
  subscribeSetupChanges(listener: () => void): () => void;

  /** Copy one of a tool's own skills or MCP servers into the building's setup; resolves to the new view. */
  importSetupItem(from: ToolKind, kind: SetupItemKind, name: string): Promise<SetupView>;

  /** Aim one of the building's setup items at `targets`; resolves to the new view. */
  setSetupTargets(kind: SetupItemKind, name: string, targets: Targets): Promise<SetupView>;

  /** Add an employee: spawn a new agent session on a project. */
  spawnSession(projectId: string, request: HireRequest): Promise<Session>;

  /** Open a plain login shell in the floor's folder (Cmd+N). Its id takes the terminal calls. */
  openShell(projectId: string): Promise<Shell>;

  /** Hang up a plain shell. The snapshot drops it once it has exited. */
  endShell(shellId: string): Promise<void>;

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

  /**
   * Pull a floor down: its running agents are stopped cleanly first, then it
   * stays hidden until a new session starts there. Rejects, leaving the floor
   * standing, when an agent will not stop.
   */
  demolishFloor(projectId: string): Promise<void>;

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

  /** A chat desk's (structured, or Claude Code in an app terminal) conversation so far. */
  structuredBacklog(sessionId: string): Promise<ConversationBacklog>;

  /** Hear every change to a chat desk. Returns an unsubscribe. */
  subscribeStructured(sessionId: string, listener: (update: ConversationUpdate) => void): () => void;

  /** Send the human's message to a chat desk (structured, or Claude Code in an app terminal). It waits when the desk is busy. */
  structuredPrompt(sessionId: string, text: string): Promise<void>;

  /** Answer a structured desk's open approval with one of its choices and the fields' values, by field id. */
  structuredAnswer(sessionId: string, approvalId: string, choiceId: string, values: Readonly<Record<string, string>>): Promise<void>;

  /** Stop the turn a chat desk is running. */
  structuredCancel(sessionId: string): Promise<void>;

  /** Withdraw one of your own messages that is still waiting for its desk. */
  withdrawMessage(messageId: string): Promise<void>;

  /** End a `structured` desk's agent; the desk leaves once it has stopped. */
  structuredStop(sessionId: string): Promise<void>;

  /** Send keystrokes to a `full`-control session's terminal. */
  writeTerminal(sessionId: string, data: string): Promise<void>;

  /** Tell a `full`-control session's terminal its new size in cells. */
  resizeTerminal(sessionId: string, cols: number, rows: number): Promise<void>;

  /** Hear about files dropped onto the window. Returns an unsubscribe. */
  subscribeFileDrops(listener: (drop: FileDrop) => void): () => void;

  /** What the building knows about the Linear key. May read the Keychain. Never returns the key. */
  linearKeyStatus(): Promise<LinearKeyStatus>;

  /**
   * Save a Linear API key (checked with Linear first) or, with null, remove the saved one.
   * Rejects, saving nothing, when Linear refuses the key or cannot be reached.
   */
  setLinearKey(key: string | null): Promise<LinearKeyStatus>;

  /** The Linear teams the key can see, each with its open projects. Rejects without a usable key. */
  linearTeams(): Promise<readonly LinearTeam[]>;

  /** Feed a floor from a connector, replacing its link of the same kind. The floor becomes pinned. */
  connectFloor(projectId: string, link: FloorLink): Promise<void>;

  /** Stop feeding a floor from a connector. Its items leave the boards. */
  disconnectFloor(projectId: string, connector: ConnectorKind): Promise<void>;

  /** Ask every connector again now, without waiting for the next minute. */
  recheckConnectors(): Promise<void>;

  /** Open a connector item (a Linear issue) in the browser. Rejects for an item with no link. */
  openTodoLink(todoId: string): Promise<void>;

  /** Switch a floor's QA desk on or off. Switching it on pins the floor. */
  setFloorQa(projectId: string, enabled: boolean): Promise<void>;

  /** Ask GitHub for pull requests again now, without waiting for the next check. */
  recheckQa(): Promise<void>;

  /** Open a QA review (or its pull request, when nothing was posted) in the browser. */
  openReview(projectId: string, number: number, head: string): Promise<void>;

  /** Hire this floor's manager. Rejects when one is already there or it cannot be started. */
  hireManager(projectId: string): Promise<void>;

  /** Pause or resume a floor's manager. Paused, it can read but nothing is sent or hired. */
  setManagerPaused(projectId: string, paused: boolean): Promise<void>;

  /** Let a floor's manager hire desks, or not. Off until switched on. */
  setManagerHiring(projectId: string, allowed: boolean): Promise<void>;

  /** Withdraw a note the manager has waiting for a desk. */
  withdrawManagerNote(projectId: string, noteId: string): Promise<void>;
}
