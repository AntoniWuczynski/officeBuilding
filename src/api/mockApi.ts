import type {
  AgentMessage,
  Decision,
  OfficeSnapshot,
  Project,
  Session,
  SessionState,
  ToolOptions,
} from "../types";
import type { DiscoveryApi, HireFailure, HireRequest, QuestionAnswer, TerminalHandlers } from "./DiscoveryApi";
import { makeSeedSnapshot } from "./mockData";
import { makeHireOptions } from "./mockHireOptions";

type Listener = (snapshot: OfficeSnapshot) => void;

/** The demo stand-in for an in-app terminal. */
interface FakeShell {
  backlog: string;
  /** The line being typed. */
  line: string;
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
  private readonly shells = new Map<string, FakeShell>();

  constructor(seed: OfficeSnapshot = makeSeedSnapshot(), now: () => Date = () => new Date()) {
    this.snapshot = seed;
    this.now = now;
  }

  hireOptions(): Promise<readonly ToolOptions[]> {
    return Promise.resolve(this.options);
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
    const model = this.options
      .find((o) => o.tool === request.tool)
      ?.models.find((m) => m.id === request.model);
    if (model === undefined) {
      return Promise.reject(new Error(`${request.model} is not a model the app can start for ${request.tool}`));
    }
    if (!model.efforts.some((e) => e.id === request.effort)) {
      return Promise.reject(new Error(`${model.label} does not support ${request.effort} effort`));
    }
    const id = this.nextId("s-new");
    const session: Session = {
      id,
      tool: request.tool,
      projectId,
      title: request.title.trim() === "" ? "New task" : request.title.trim(),
      state: "thinking",
      control: "full",
      model: model.id,
      effort: request.effort,
      lastActivityAt: this.stamp(),
      pendingQuestionIds: [],
      spend: { usd: 0, tokens: 0, unpricedTokens: 0 },
    };
    const projects = this.snapshot.projects.map((p) =>
      p.id === projectId ? { ...p, sessionIds: [...p.sessionIds, id] } : p,
    );
    this.commit({
      ...this.snapshot,
      projects,
      sessions: [...this.snapshot.sessions, session],
    });
    return Promise.resolve(session);
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
    handlers.output(shell.backlog);
    shell.watchers.add(handlers);
    return () => {
      shell.watchers.delete(handlers);
    };
  }

  /** The demo's hires always reach their desks, so there is nothing to report. */
  subscribeHireFailures(_listener: (failure: HireFailure) => void): () => void {
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
    return Promise.resolve();
  }

  /** The demo terminal of an app-controlled session, started on first use. */
  private shell(sessionId: string): FakeShell | Error {
    const session = this.snapshot.sessions.find((s) => s.id === sessionId);
    if (session === undefined) return new Error(`unknown session: ${sessionId}`);
    if (session.control !== "full") return new Error(`${session.title} was not started in the app, so it has no terminal here`);
    let shell = this.shells.get(sessionId);
    if (shell === undefined) {
      shell = { backlog: `Demo terminal for “${session.title}”. Type a line and press Enter.\r\n$ `, line: "", watchers: new Set() };
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
