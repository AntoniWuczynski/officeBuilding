import type {
  AgentMessage,
  ConversationBacklog,
  ConversationUpdate,
  Decision,
  OfficeSnapshot,
  Project,
  Session,
  SessionState,
  SetupItemKind,
  SetupView,
  Targets,
  ToolKind,
  ToolOptions,
  ToolUsage,
} from "../types";
import type { DiscoveryApi, FileDrop, HireFailure, HireRequest, QuestionAnswer, TerminalHandlers, TerminalSize } from "./DiscoveryApi";
import { makeSeedSnapshot } from "./mockData";
import { makeHireOptions } from "./mockHireOptions";
import { makeSetupView } from "./mockSetup";
import { MockConversations } from "./mockConversation";

type Listener = (snapshot: OfficeSnapshot) => void;

/** The demo stand-in for an in-app terminal. */
interface FakeShell {
  backlog: string;
  /** The line being typed. */
  line: string;
  /** The size the panel last gave it, which its backlog is drawn at. */
  size: TerminalSize;
  readonly watchers: Set<TerminalHandlers>;
}

/** Keep the visible message history short; the UI only animates new ones. */
const MESSAGE_HISTORY = 20;

/** Working-state cycle the demo simulation walks sessions through. */
const NEXT_STATE: Partial<Record<SessionState, SessionState>> = {
  idle: "thinking",
  thinking: "running",
  running: "thinking",
};

const DEMO_NOTES: readonly string[] = [
  "Can you rebase onto my branch before you push?",
  "Schema changed: EventType is now required.",
  "I need your fixture file for the parity test.",
  "Heads up, I am touching the same module.",
];

/**
 * In-browser implementation of {@link DiscoveryApi} for Phase 1. Holds a
 * mutable snapshot, notifies subscribers on every mutation, and simulates the
 * actions the real backend will perform. No native dependencies — the whole UI
 * runs and is verifiable in a plain browser / jsdom.
 */
export class MockApi implements DiscoveryApi {
  private snapshot: OfficeSnapshot;
  private readonly listeners = new Set<Listener>();
  private seq = 0;
  private readonly now: () => Date;
  private readonly options: readonly ToolOptions[] = makeHireOptions();
  private setup: SetupView = makeSetupView();
  /** A structured desk's state follows its conversation; one that ends leaves its floor. */
  private readonly conversations = new MockConversations((sessionId, state) => {
    if (state === null) {
      this.commit({
        ...this.snapshot,
        sessions: this.snapshot.sessions.filter((x) => x.id !== sessionId),
        projects: this.snapshot.projects.map((p) => ({ ...p, sessionIds: p.sessionIds.filter((x) => x !== sessionId) })),
      });
      return;
    }
    this.commit({ ...this.snapshot, sessions: this.snapshot.sessions.map((x) => (x.id === sessionId ? { ...x, state } : x)) });
  });
  private readonly shells = new Map<string, FakeShell>();

  constructor(seed: OfficeSnapshot = makeSeedSnapshot(), now: () => Date = () => new Date()) {
    this.snapshot = seed;
    this.now = now;
  }

  setupView(): Promise<SetupView> {
    return Promise.resolve(this.setup);
  }

  /** Copies get the names src-tauri/src/setup/import.rs gives them. */
  importSetupItem(from: ToolKind, kind: SetupItemKind, name: string): Promise<SetupView> {
    const harness = this.setup.harnesses.find((h) => h.tool === from);
    const hook = harness?.hooks?.find((h) => h.id === name);
    const found: { readonly name: string; readonly description: string } | undefined =
      kind === "skill"
        ? harness?.skills?.find((s) => s.name === name)
        : kind === "mcp-server"
          ? harness?.mcpServers?.includes(name) === true
            ? { name, description: "npx" }
            : undefined
          : kind === "instructions"
            ? harness?.instructions?.includes(name) === true
              ? { name: `${from}-instructions`, description: `From ${name}` }
              : undefined
            : hook === undefined
              ? undefined
              : { name: `${from}-${name.replace(":", "-").toLowerCase()}`, description: `${hook.event}: ${hook.command}` };
    if (found === undefined) return Promise.reject(new Error(`${name} is not one of that tool's own`));
    if (this.setup.items.some((i) => i.kind === kind && i.name === found.name)) return Promise.reject(new Error(`the building already has ${found.name}`));
    const tools = this.setup.harnesses.filter((h) => h.tool !== from && h.support[kind].kind === "supported").map((h) => h.tool);
    this.setup = { ...this.setup, items: [...this.setup.items, { kind, name: found.name, description: found.description, targets: { kind: "only", tools } }] };
    return Promise.resolve(this.setup);
  }

  setSetupTargets(kind: SetupItemKind, name: string, targets: Targets): Promise<SetupView> {
    if (!this.setup.items.some((i) => i.kind === kind && i.name === name)) {
      return Promise.reject(new Error(`there is no ${kind} called ${name}`));
    }
    this.setup = { ...this.setup, items: this.setup.items.map((i) => (i.kind === kind && i.name === name ? { ...i, targets } : i)) };
    return Promise.resolve(this.setup);
  }

  hireOptions(): Promise<readonly ToolOptions[]> {
    return Promise.resolve(this.options);
  }

  toolUsage(): Promise<readonly ToolUsage[]> {
    const ago = (minutes: number): string => new Date(this.now().getTime() - minutes * 60_000).toISOString();
    const ahead = (hours: number): string => new Date(this.now().getTime() + hours * 3_600_000).toISOString();
    return Promise.resolve([
      {
        tool: "claude-code",
        checkedAt: ago(20),
        unavailable: null,
        windows: [
          { id: "session", label: "Session", percent: 8, resetsAt: ahead(3) },
          { id: "week", label: "Week", percent: 40, resetsAt: ahead(72) },
          { id: "week-fable", label: "Week · Fable", percent: 12, resetsAt: ahead(72) },
        ],
      },
      {
        tool: "codex",
        checkedAt: ago(1),
        unavailable: null,
        windows: [
          { id: "5h", label: "5-hour", percent: 5, resetsAt: ahead(4) },
          { id: "week", label: "Week", percent: 9, resetsAt: ahead(96) },
        ],
      },
      { tool: "antigravity", checkedAt: null, windows: [], unavailable: "Antigravity keeps its quota on Google's servers, not on this Mac" },
    ]);
  }

  getSnapshot(): Promise<OfficeSnapshot> {
    return Promise.resolve(this.snapshot);
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    listener(this.snapshot);
    return () => {
      this.listeners.delete(listener);
    };
  }

  spawnSession(projectId: string, request: HireRequest): Promise<Session> {
    const project = this.snapshot.projects.find((p) => p.id === projectId);
    if (project === undefined) {
      return Promise.reject(new Error(`unknown project: ${projectId}`));
    }
    const tool = this.options.find((o) => o.tool === request.tool);
    const model = tool?.models.find((m) => m.id === request.model);
    if (model === undefined) {
      return Promise.reject(new Error(`${request.model} is not a model the app can start for ${request.tool}`));
    }
    if (!model.efforts.some((e) => e.id === request.effort)) {
      return Promise.reject(new Error(`${model.label} does not support ${request.effort} effort`));
    }
    if (request.ultracode && tool?.ultracode === null) {
      return Promise.reject(new Error(`${request.tool} has no ultracode`));
    }
    if (request.structured && tool?.structured !== true) {
      return Promise.reject(new Error(`${request.tool} can only run in a terminal`));
    }
    const id = this.nextId("s-new");
    const session: Session = {
      id,
      tool: request.tool,
      projectId,
      title: request.title.trim() === "" ? "New task" : request.title.trim(),
      state: "thinking",
      control: request.structured ? "structured" : "full",
      model: model.id,
      effort: request.effort,
      lastActivityAt: this.stamp(),
      pendingQuestionIds: [],
      spend: { usd: 0, tokens: 0, unpricedTokens: 0 },
      helpers: [],
      worktree: false,
    };
    const projects = this.snapshot.projects.map((p) =>
      p.id === projectId ? { ...p, sessionIds: [...p.sessionIds, id] } : p,
    );
    this.commit({
      ...this.snapshot,
      projects,
      sessions: [...this.snapshot.sessions, session],
    });
    if (request.structured) this.conversations.start(id, request.title.trim());
    return Promise.resolve(session);
  }

  structuredBacklog(sessionId: string): Promise<ConversationBacklog> {
    const b = this.conversations.backlog(sessionId);
    return b instanceof Error ? Promise.reject(b) : Promise.resolve(b);
  }

  subscribeStructured(sessionId: string, listener: (update: ConversationUpdate) => void): () => void {
    return this.conversations.subscribe(sessionId, listener);
  }

  structuredPrompt(sessionId: string, text: string): Promise<void> {
    const e = this.conversations.prompt(sessionId, text.trim());
    return e === null ? Promise.resolve() : Promise.reject(e);
  }

  structuredAnswer(sessionId: string, approvalId: string, choiceId: string, values: Readonly<Record<string, string>>): Promise<void> {
    const e = this.conversations.answer(sessionId, approvalId, choiceId, values);
    return e === null ? Promise.resolve() : Promise.reject(e);
  }

  structuredCancel(sessionId: string): Promise<void> {
    this.conversations.cancel(sessionId);
    return Promise.resolve();
  }

  structuredStop(sessionId: string): Promise<void> {
    this.conversations.stop(sessionId);
    return Promise.resolve();
  }

  /** The agent walks back in at once, thinking, in its own (fake) terminal. */
  resumeSession(sessionId: string): Promise<void> {
    const ended = this.snapshot.ended.find((e) => e.id === sessionId);
    if (ended === undefined) {
      return Promise.reject(new Error(`${sessionId} has not signed out, so there is nothing to resume`));
    }
    const session: Session = {
      id: ended.id,
      tool: ended.tool,
      projectId: ended.projectId,
      title: ended.title,
      state: "thinking",
      control: "full",
      model: ended.model,
      effort: ended.effort,
      lastActivityAt: this.stamp(),
      pendingQuestionIds: [],
      spend: { usd: 0, tokens: 0, unpricedTokens: 0 },
      helpers: [],
      worktree: false,
    };
    this.commit({
      ...this.snapshot,
      projects: this.snapshot.projects.map((p) =>
        p.id === ended.projectId ? { ...p, sessionIds: [...p.sessionIds, ended.id] } : p,
      ),
      sessions: [...this.snapshot.sessions, session],
      ended: this.snapshot.ended.filter((e) => e.id !== sessionId),
    });
    return Promise.resolve();
  }

  answerQuestion(questionId: string, answer: QuestionAnswer): Promise<void> {
    const question = this.snapshot.questions.find((q) => q.id === questionId);
    if (question === undefined) {
      return Promise.reject(new Error(`unknown question: ${questionId}`));
    }
    let chosen: string;
    if (answer.kind === "option") {
      const option = question.options.find((o) => o.id === answer.optionId);
      if (option === undefined) {
        return Promise.reject(
          new Error(`unknown option ${answer.optionId} for ${questionId}`),
        );
      }
      chosen = option.label;
    } else {
      if (answer.text.trim() === "") {
        return Promise.reject(new Error("empty free-text answer"));
      }
      chosen = answer.text.trim();
    }
    if (question.answerVia === "terminal") {
      return Promise.reject(new Error("this agent was started outside the app; answer it in its own terminal"));
    }
    const questions = this.snapshot.questions.filter((q) => q.id !== questionId);
    // Unblock the asking session once its last question is answered.
    const sessions = this.snapshot.sessions.map((s) => {
      if (s.id !== question.sessionId) return s;
      const pendingQuestionIds = s.pendingQuestionIds.filter((qid) => qid !== questionId);
      return {
        ...s,
        state: pendingQuestionIds.length === 0 ? ("running" as const) : s.state,
        pendingQuestionIds,
        lastActivityAt: this.stamp(),
      };
    });
    const decisions: Decision[] = [
      ...this.snapshot.decisions,
      {
        // "answer-" keeps minted ids clear of the seed's d-1, d-2 ...
        id: this.nextId("d-answer"),
        projectId: question.projectId,
        title: chosen,
        detail: question.prompt,
        decidedAt: this.stamp(),
        by: "human",
      },
    ];
    this.commit({ ...this.snapshot, questions, sessions, decisions });
    return Promise.resolve();
  }

  tickTodo(todoId: string): Promise<void> {
    const exists = this.snapshot.todos.some((t) => t.id === todoId);
    if (!exists) {
      return Promise.reject(new Error(`unknown todo: ${todoId}`));
    }
    const todos = this.snapshot.todos.map((t) =>
      t.id === todoId ? { ...t, done: true } : t,
    );
    this.commit({ ...this.snapshot, todos });
    return Promise.resolve();
  }

  sendAgentMessage(
    fromSessionId: string,
    toSessionId: string,
    text: string,
  ): Promise<void> {
    const known = (id: string): boolean =>
      this.snapshot.sessions.some((s) => s.id === id);
    if (!known(fromSessionId) || !known(toSessionId)) {
      return Promise.reject(new Error("unknown session in agent message"));
    }
    if (fromSessionId === toSessionId) {
      return Promise.reject(new Error("an agent cannot message itself"));
    }
    if (text.trim() === "") {
      return Promise.reject(new Error("empty agent message"));
    }
    const message: AgentMessage = {
      id: this.nextId("m"),
      fromSessionId,
      toSessionId,
      text: text.trim(),
      sentAt: this.stamp(),
    };
    this.commit({
      ...this.snapshot,
      messages: [...this.snapshot.messages, message].slice(-MESSAGE_HISTORY),
    });
    return Promise.resolve();
  }

  focusSession(sessionId: string): Promise<void> {
    const session = this.snapshot.sessions.find((s) => s.id === sessionId);
    if (session === undefined) {
      return Promise.reject(new Error(`unknown session: ${sessionId}`));
    }
    if (session.control !== "raise-window") {
      return Promise.reject(
        new Error(`${session.title} runs inside the app; there is no window to raise`),
      );
    }
    // Mock: nothing to raise in a browser. The real backend activates the
    // owning terminal app's window (plan §11.1).
    return Promise.resolve();
  }

  addFloor(path: string): Promise<Project> {
    const clean = path.trim().replace(/\/+$/, "");
    if (!clean.startsWith("/") && !clean.startsWith("~/")) {
      return Promise.reject(new Error("give the full path to the folder, for example ~/code/app"));
    }
    const existing = this.snapshot.projects.find((p) => p.path === clean);
    if (existing !== undefined) return Promise.resolve(existing);
    const name = clean.split("/").pop() ?? clean;
    const project: Project = { id: this.nextId("p"), name, path: clean, sessionIds: [] };
    this.commit({ ...this.snapshot, projects: [...this.snapshot.projects, project] });
    return Promise.resolve(project);
  }

  /** The demo's agents always stop when asked, so the floor always comes down. */
  demolishFloor(projectId: string): Promise<void> {
    if (!this.snapshot.projects.some((p) => p.id === projectId)) {
      return Promise.reject(new Error(`${projectId} is not a floor of this building`));
    }
    const leaving = new Set(this.snapshot.sessions.filter((s) => s.projectId === projectId).map((s) => s.id));
    for (const id of leaving) this.shells.delete(id);
    this.commit({
      ...this.snapshot,
      projects: this.snapshot.projects.filter((p) => p.id !== projectId),
      sessions: this.snapshot.sessions.filter((s) => !leaving.has(s.id)),
      ended: this.snapshot.ended.filter((e) => e.projectId !== projectId),
      questions: this.snapshot.questions.filter((q) => q.projectId !== projectId),
    });
    return Promise.resolve();
  }

  /** A browser has no native folder picker; the dialog asks for a typed path. */
  readonly canPickFolder: boolean = false;

  pickFolder(): Promise<string | null> {
    return Promise.resolve(null);
  }

  subscribeTerminal(sessionId: string, handlers: TerminalHandlers): () => void {
    const shell = this.shell(sessionId);
    if (shell instanceof Error) {
      handlers.error(shell.message);
      return () => {};
    }
    handlers.output(shell.backlog, shell.size);
    shell.watchers.add(handlers);
    return () => {
      shell.watchers.delete(handlers);
    };
  }

  /** The demo's hires always reach their desks, so there is nothing to report. */
  /** The demo's setup only changes through the panel itself. */
  subscribeSetupChanges(_listener: () => void): () => void {
    return () => {};
  }

  subscribeHireFailures(_listener: (failure: HireFailure) => void): () => void {
    return () => {};
  }

  /** A browser page gets no file paths from a drop, so the demo hears none. */
  subscribeFileDrops(_listener: (drop: FileDrop) => void): () => void {
    return () => {};
  }

  /** A tiny fake shell: echoes what is typed and answers each line. */
  writeTerminal(sessionId: string, data: string): Promise<void> {
    const shell = this.shell(sessionId);
    if (shell instanceof Error) return Promise.reject(shell);
    let out = "";
    for (const ch of data) {
      if (ch === "\r" || ch === "\n") {
        const line = shell.line.trim();
        out += `\r\n${line === "" ? "" : `echo: ${line}\r\n`}$ `;
        shell.line = "";
      } else if (ch === "\u007f") {
        if (shell.line !== "") {
          shell.line = shell.line.slice(0, -1);
          out += "\b \b";
        }
      } else if (ch >= " ") {
        shell.line += ch;
        out += ch;
      }
    }
    shell.backlog += out;
    for (const w of shell.watchers) w.output(out);
    return Promise.resolve();
  }

  resizeTerminal(sessionId: string, cols: number, rows: number): Promise<void> {
    const shell = this.shell(sessionId);
    if (shell instanceof Error) return Promise.reject(shell);
    if (cols < 1 || rows < 1) return Promise.reject(new Error("a terminal needs at least one row and one column"));
    shell.size = { cols, rows };
    return Promise.resolve();
  }

  /** The demo terminal of an app-controlled session, started on first use. */
  private shell(sessionId: string): FakeShell | Error {
    const session = this.snapshot.sessions.find((s) => s.id === sessionId);
    if (session === undefined) return new Error(`unknown session: ${sessionId}`);
    if (session.control !== "full") return new Error(`${session.title} was not started in the app, so it has no terminal here`);
    let shell = this.shells.get(sessionId);
    if (shell === undefined) {
      shell = { backlog: `Demo terminal for “${session.title}”. Type a line and press Enter.\r\n$ `, line: "", size: { cols: 80, rows: 24 }, watchers: new Set() };
      this.shells.set(sessionId, shell);
    }
    return shell;
  }

  /**
   * Demo only: every `intervalMs`, nudge one working session to its next state
   * and now and then have two colleagues exchange a note, so the office looks
   * alive without a backend. Returns a stop function.
   */
  startSimulation(random: () => number = Math.random, intervalMs = 4000): () => void {
    const timer = setInterval(() => {
      this.simulateTick(random);
    }, intervalMs);
    return () => {
      clearInterval(timer);
    };
  }

  /** One simulation step; public so tests can drive it deterministically. */
  simulateTick(random: () => number): void {
    const roll = random();
    if (roll < 0.7) {
      const movable = this.snapshot.sessions.filter((s) => NEXT_STATE[s.state] !== undefined);
      const target = movable[Math.floor(random() * movable.length)];
      const next = target === undefined ? undefined : NEXT_STATE[target.state];
      if (target === undefined || next === undefined) return;
      this.commit({
        ...this.snapshot,
        sessions: this.snapshot.sessions.map((s) =>
          s.id === target.id ? { ...s, state: next, lastActivityAt: this.stamp() } : s,
        ),
      });
      return;
    }
    // Pick two awake colleagues on the same floor.
    const awake = this.snapshot.sessions.filter((s) => s.state !== "done" && s.state !== "error");
    const from = awake[Math.floor(random() * awake.length)];
    if (from === undefined) return;
    const colleagues = awake.filter((s) => s.projectId === from.projectId && s.id !== from.id);
    const to = colleagues[Math.floor(random() * colleagues.length)];
    const note = DEMO_NOTES[Math.floor(random() * DEMO_NOTES.length)];
    if (to === undefined || note === undefined) return;
    void this.sendAgentMessage(from.id, to.id, note);
  }

  private nextId(prefix: string): string {
    this.seq += 1;
    return `${prefix}-${this.seq}`;
  }

  private stamp(): string {
    return this.now().toISOString();
  }

  private commit(next: OfficeSnapshot): void {
    this.snapshot = next;
    for (const listener of this.listeners) {
      listener(next);
    }
  }
}
