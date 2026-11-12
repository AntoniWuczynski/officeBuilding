import type { Approval, ChatCapabilities, ConversationBacklog, ConversationEntry, ConversationUpdate, QueuedNote, SessionState } from "../types";

/** What a structured desk offers: everything but a terminal. */
export const STRUCTURED: ChatCapabilities = { compose: true, stop: true, end: true, approvals: "inline", terminal: false };

/** A Claude Code desk in one of the app's terminals: it can be typed to and stopped, and answers are given in the terminal. */
export const CLAUDE_TERMINAL: ChatCapabilities = { compose: true, stop: true, end: false, approvals: "terminal", terminal: true };

type Listener = (update: ConversationUpdate) => void;

interface Desk {
  transcript: ConversationEntry[];
  state: SessionState;
  approval: Approval | null;
  ended: string | null;
  readonly capabilities: ChatCapabilities;
  /** Your messages sent while a turn was open, oldest first. */
  waiting: QueuedNote[];
  readonly listeners: Set<Listener>;
}

/**
 * Structured desks for the browser build: each prompt gets a short answer, and
 * one that asks to run something gets an approval first, as a real agent would.
 */
export class MockConversations {
  private readonly desks = new Map<string, Desk>();
  private seq = 0;

  constructor(private readonly onDeskChanged: (sessionId: string, state: SessionState | null) => void) {}

  start(sessionId: string, task: string, capabilities: ChatCapabilities = STRUCTURED): void {
    this.desks.set(sessionId, { transcript: [], state: "idle", approval: null, ended: null, capabilities, waiting: [], listeners: new Set() });
    if (task !== "") this.prompt(sessionId, task);
  }

  has(sessionId: string): boolean {
    return this.desks.has(sessionId);
  }

  backlog(sessionId: string): ConversationBacklog | Error {
    const d = this.desks.get(sessionId);
    if (d === undefined) return new Error(`${sessionId} is not a desk the app is running`);
    return { transcript: [...d.transcript], state: d.state, approval: d.approval, ended: d.ended, capabilities: d.capabilities, earlier: false };
  }

  subscribe(sessionId: string, listener: Listener): () => void {
    this.desks.get(sessionId)?.listeners.add(listener);
    return () => {
      this.desks.get(sessionId)?.listeners.delete(listener);
    };
  }

  prompt(sessionId: string, text: string): Error | null {
    const d = this.desks.get(sessionId);
    if (d === undefined || d.ended !== null) return new Error(`${sessionId} is not a desk the app is running`);
    if (d.state === "thinking" || d.state === "waiting-human") {
      this.seq += 1;
      d.waiting.push({ id: `q-${this.seq}`, toSessionId: sessionId, text, queuedAt: new Date().toISOString(), hold: "busy" });
      this.onDeskChanged(sessionId, d.state);
      return null;
    }
    this.push(sessionId, { kind: "user", id: this.id(), text });
    this.setState(sessionId, "thinking");
    if (d.capabilities.approvals === "terminal" && /\b(ask|run)\b/i.test(text)) {
      d.approval = { id: "terminal-prompt", title: "Claude Code is waiting for your answer", detail: "Claude needs your permission to use Bash", fields: [], choices: [] };
      this.setState(sessionId, "waiting-human");
    } else if (/\bask\b/i.test(text)) {
      d.approval = {
        id: this.id(),
        title: "Which database?",
        detail: "",
        fields: [{ id: "db", label: "Database", kind: { kind: "pick", options: ["Postgres", "SQLite"], other: true }, required: true }],
        choices: [{ id: "send", label: "Send", allow: true }],
      };
      this.setState(sessionId, "waiting-human");
    } else if (/\brun\b/i.test(text)) {
      d.approval = {
        id: this.id(),
        title: "Run a command",
        detail: "echo hi",
        fields: [],
        choices: [
          { id: "allow", label: "Allow once", allow: true },
          { id: "deny", label: "Decline", allow: false },
        ],
      };
      this.setState(sessionId, "waiting-human");
    } else if (/\bedit\b/i.test(text)) {
      this.push(sessionId, {
        kind: "tool",
        id: this.id(),
        title: "Edit notes.txt",
        status: "done",
        detail: "",
        diffs: [{ path: "notes.txt", text: "-old line\n+new line" }],
      });
      this.reply(sessionId, "Edited it:\n\n- one line changed\n- nothing else touched");
    } else {
      this.reply(sessionId, `Done: ${text}`);
    }
    return null;
  }

  answer(sessionId: string, approvalId: string, choiceId: string, values: Readonly<Record<string, string>>): Error | null {
    const d = this.desks.get(sessionId);
    if (d?.approval?.id !== approvalId) return new Error("that approval is no longer open");
    const allowed = d.approval.choices.find((c) => c.id === choiceId)?.allow ?? false;
    const missing = d.approval.fields.find((f) => f.required && (values[f.id] ?? "").trim() === "");
    if (allowed && missing !== undefined) return new Error(`${missing.label} needs an answer first`);
    const asked = d.approval.fields.length > 0;
    d.approval = null;
    if (asked) {
      this.reply(sessionId, `Going with ${Object.values(values).join(", ")}.`);
      return null;
    }
    if (allowed) this.push(sessionId, { kind: "tool", id: this.id(), title: "echo hi", status: "done", detail: "hi", diffs: [] });
    this.reply(sessionId, allowed ? "It printed hi." : "All right, I left it.");
    return null;
  }

  /** A line break typed in a Claude Code desk's terminal settles its terminal card. */
  answeredInTerminal(sessionId: string, typed: string): void {
    const d = this.desks.get(sessionId);
    if (d?.approval?.id !== "terminal-prompt" || !/[\r\n]/.test(typed)) return;
    d.approval = null;
    this.reply(sessionId, "It ran.");
  }

  /** Every message still waiting for its desk, oldest first. */
  queued(): QueuedNote[] {
    return [...this.desks.values()].flatMap((d) => d.waiting.map((w) => ({ ...w, hold: d.approval === null ? w.hold : ("waiting-on-you" as const) })));
  }

  withdraw(messageId: string): Error | null {
    for (const [sessionId, d] of this.desks) {
      if (!d.waiting.some((w) => w.id === messageId)) continue;
      d.waiting = d.waiting.filter((w) => w.id !== messageId);
      this.onDeskChanged(sessionId, d.state);
      return null;
    }
    return new Error("that message is no longer waiting");
  }

  cancel(sessionId: string): void {
    const d = this.desks.get(sessionId);
    if (d === undefined) return;
    d.approval = null;
    this.push(sessionId, { kind: "notice", id: this.id(), text: "Turn stopped." });
    this.setState(sessionId, "idle");
  }

  stop(sessionId: string): Error | null {
    const d = this.desks.get(sessionId);
    if (d === undefined) return null;
    if (!d.capabilities.end) return new Error("only a structured desk can be ended from its conversation");
    d.waiting = [];
    d.ended = "The agent has finished.";
    d.approval = null;
    this.emit(sessionId, null);
    this.onDeskChanged(sessionId, null);
    return null;
  }

  private reply(sessionId: string, text: string): void {
    this.push(sessionId, { kind: "agent", id: this.id(), text });
    this.setState(sessionId, "idle");
  }

  private id(): string {
    this.seq += 1;
    return `m-${this.seq}`;
  }

  private push(sessionId: string, entry: ConversationEntry): void {
    this.desks.get(sessionId)?.transcript.push(entry);
    this.emit(sessionId, entry);
  }

  private setState(sessionId: string, state: SessionState): void {
    const d = this.desks.get(sessionId);
    if (d === undefined) return;
    d.state = state;
    this.emit(sessionId, null);
    this.onDeskChanged(sessionId, state);
    // The turn is over: the oldest waiting message is sent as the next one.
    const next = state === "idle" ? d.waiting.shift() : undefined;
    if (next !== undefined) this.prompt(sessionId, next.text);
  }

  private emit(sessionId: string, entry: ConversationEntry | null): void {
    const d = this.desks.get(sessionId);
    if (d === undefined) return;
    const update: ConversationUpdate = { sessionId, entry, state: d.state, approval: d.approval, ended: d.ended, reset: false };
    for (const l of d.listeners) l(update);
  }
}
