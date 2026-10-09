import type { Approval, ConversationBacklog, ConversationEntry, ConversationUpdate, SessionState } from "../types";

type Listener = (update: ConversationUpdate) => void;

interface Desk {
  transcript: ConversationEntry[];
  state: SessionState;
  approval: Approval | null;
  ended: string | null;
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

  start(sessionId: string, task: string): void {
    this.desks.set(sessionId, { transcript: [], state: "idle", approval: null, ended: null, listeners: new Set() });
    if (task !== "") this.prompt(sessionId, task);
  }

  has(sessionId: string): boolean {
    return this.desks.has(sessionId);
  }

  backlog(sessionId: string): ConversationBacklog | Error {
    const d = this.desks.get(sessionId);
    if (d === undefined) return new Error(`${sessionId} is not a desk the app is running`);
    return { transcript: [...d.transcript], state: d.state, approval: d.approval, ended: d.ended };
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
    if (d.state === "thinking" || d.state === "waiting-human") return new Error("the agent is still on the last turn: wait, or stop it");
    this.push(sessionId, { kind: "user", id: this.id(), text });
    this.setState(sessionId, "thinking");
    if (/\bask\b/i.test(text)) {
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
    if (allowed) this.push(sessionId, { kind: "tool", id: this.id(), title: "echo hi", status: "done", detail: "hi" });
    this.reply(sessionId, allowed ? "It printed hi." : "All right, I left it.");
    return null;
  }

  cancel(sessionId: string): void {
    const d = this.desks.get(sessionId);
    if (d === undefined) return;
    d.approval = null;
    this.push(sessionId, { kind: "notice", id: this.id(), text: "Turn stopped." });
    this.setState(sessionId, "idle");
  }

  stop(sessionId: string): void {
    const d = this.desks.get(sessionId);
    if (d === undefined) return;
    d.ended = "The agent has finished.";
    d.approval = null;
    this.emit(sessionId, null);
    this.onDeskChanged(sessionId, null);
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
  }

  private emit(sessionId: string, entry: ConversationEntry | null): void {
    const d = this.desks.get(sessionId);
    if (d === undefined) return;
    const update: ConversationUpdate = { sessionId, entry, state: d.state, approval: d.approval, ended: d.ended };
    for (const l of d.listeners) l(update);
  }
}
