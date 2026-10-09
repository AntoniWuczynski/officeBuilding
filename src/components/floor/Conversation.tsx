import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { DiscoveryApi } from "../../api/DiscoveryApi";
import type { Approval, ApprovalField, ConversationEntry, ConversationUpdate, SessionState } from "../../types";
import { message } from "../../lib/errors";

export type ConversationApi = Pick<
  DiscoveryApi,
  "structuredBacklog" | "subscribeStructured" | "structuredPrompt" | "structuredAnswer" | "structuredCancel" | "structuredStop"
>;

/** The structured-desk operations, provided by App beside the terminal ones. */
export const ConversationApiContext = createContext<ConversationApi | null>(null);

interface View {
  readonly transcript: readonly ConversationEntry[];
  readonly state: SessionState;
  readonly approval: Approval | null;
  readonly ended: string | null;
}

/** Add an entry, or replace the one with the same id (streamed text grows, a tool call finishes). */
function upsert(list: readonly ConversationEntry[], entry: ConversationEntry): readonly ConversationEntry[] {
  const at = list.findIndex((e) => e.id === entry.id);
  return at === -1 ? [...list, entry] : list.map((e, i) => (i === at ? entry : e));
}

function apply(view: View, u: ConversationUpdate): View {
  return {
    transcript: u.entry === null ? view.transcript : upsert(view.transcript, u.entry),
    state: u.state,
    approval: u.approval,
    ended: u.ended,
  };
}

const TOOL_WORDS = { pending: "Waiting", running: "Running", done: "Done", failed: "Failed" } as const;

/**
 * A structured desk's conversation, in the docked panel: the transcript, an open
 * approval with its choices, and a box for the next message. The agent keeps
 * running when the panel closes.
 */
export function Conversation({ sessionId }: { readonly sessionId: string }): React.ReactElement {
  const api = useContext(ConversationApiContext);
  if (api === null) throw new Error("Conversation needs a ConversationApiContext provider");
  const [view, setView] = useState<View | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Updates that arrive before the backlog are replayed on top of it.
    const early: ConversationUpdate[] = [];
    let loaded = false;
    const stop = api.subscribeStructured(sessionId, (u) => {
      if (loaded) setView((v) => (v === null ? v : apply(v, u)));
      else early.push(u);
    });
    api.structuredBacklog(sessionId).then(
      (b) => {
        loaded = true;
        setView(early.reduce(apply, b));
      },
      (e: unknown) => setError(message(e)),
    );
    return stop;
  }, [api, sessionId]);

  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [view?.transcript.length, view?.approval]);

  const run = (call: Promise<void>): void => {
    setError(null);
    call.catch((e: unknown) => setError(message(e)));
  };
  const send = (): void => {
    const text = draft.trim();
    if (text === "") return;
    setDraft("");
    run(api.structuredPrompt(sessionId, text));
  };

  if (view === null) {
    return <p className="terminal-note">{error === null ? "Opening the conversation…" : `The conversation could not be opened: ${error}.`}</p>;
  }
  const busy = view.state === "thinking" || view.state === "running";
  return (
    <div className="conversation" data-testid="conversation">
      <ol className="conversation-log">
        {view.transcript.map((e) => (
          <Line key={e.id} entry={e} />
        ))}
        {view.transcript.length === 0 && view.ended === null ? <li className="conversation-hint">The agent is starting. Write its first task below.</li> : null}
      </ol>
      {view.approval === null ? null : (
        <ApprovalCard key={view.approval.id} approval={view.approval} onAnswer={(choiceId, values) => {
          const approval = view.approval;
          if (approval !== null) run(api.structuredAnswer(sessionId, approval.id, choiceId, values));
        }} />
      )}
      <div ref={end} />
      {error === null ? null : (
        <p className="terminal-note terminal-status" role="alert">
          {error}.
        </p>
      )}
      {view.ended !== null ? (
        <p className="terminal-note terminal-status" role="status">
          {view.ended} The conversation stays above.
        </p>
      ) : (
        <form
          className="conversation-input"
          onSubmit={(e) => {
            e.preventDefault();
            send();
          }}
        >
          <textarea
            value={draft}
            aria-label="Message to the agent"
            placeholder={busy ? "The agent is working. You can stop the turn." : "Message the agent (Enter sends, Shift+Enter adds a line)"}
            rows={2}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
          />
          <div className="conversation-actions">
            <button type="submit" className="btn btn-primary" disabled={draft.trim() === ""}>
              Send
            </button>
            {busy ? (
              <button type="button" className="btn" onClick={() => run(api.structuredCancel(sessionId))}>
                Stop turn
              </button>
            ) : null}
            <button type="button" className="btn btn-danger" onClick={() => run(api.structuredStop(sessionId))}>
              End agent
            </button>
          </div>
        </form>
      )}
    </div>
  );
}

function Line({ entry }: { readonly entry: ConversationEntry }): React.ReactElement {
  switch (entry.kind) {
    case "user":
      return <li className="conversation-user">{entry.text}</li>;
    case "agent":
      return <li className="conversation-agent">{entry.text}</li>;
    case "thought":
      return <li className="conversation-thought">{entry.text}</li>;
    case "notice":
      return <li className="conversation-notice">{entry.text}</li>;
    case "tool":
      return (
        <li className={`conversation-tool is-${entry.status}`}>
          <details>
            <summary>
              <span className="conversation-tool-status">{TOOL_WORDS[entry.status]}</span> {entry.title}
            </summary>
            {entry.detail === "" ? null : <pre>{entry.detail}</pre>}
          </details>
        </li>
      );
  }
}

/** The agent's request, its fields to fill in, and one button per answer. */
function ApprovalCard({
  approval,
  onAnswer,
}: {
  readonly approval: Approval;
  readonly onAnswer: (choiceId: string, values: Readonly<Record<string, string>>) => void;
}): React.ReactElement {
  const [values, setValues] = useState<Readonly<Record<string, string>>>({});
  const set = (id: string, value: string): void => setValues((v) => ({ ...v, [id]: value }));
  const missing = approval.fields.some((f) => f.required && (values[f.id] ?? "").trim() === "");
  return (
    <section className="conversation-approval" role="group" aria-label={`The agent asks: ${approval.title}`} data-testid="conversation-approval">
      <b>{approval.title}</b>
      {approval.detail === "" ? null : <pre>{approval.detail}</pre>}
      {approval.fields.map((f) => (
        <FieldInput key={f.id} field={f} value={values[f.id] ?? ""} onChange={(v) => set(f.id, v)} />
      ))}
      <div className="conversation-choices">
        {approval.choices.map((c) => (
          <button
            key={c.id}
            type="button"
            className={c.allow ? "btn btn-primary" : "btn"}
            disabled={c.allow && missing}
            title={c.allow && missing ? "Fill in every field marked as needed first" : undefined}
            onClick={() => onAnswer(c.id, values)}
          >
            {c.label}
          </button>
        ))}
      </div>
    </section>
  );
}

/** One field: text (hidden for a secret), a number, yes or no, or a pick-list with an optional "other". */
function FieldInput({
  field,
  value,
  onChange,
}: {
  readonly field: ApprovalField;
  readonly value: string;
  readonly onChange: (value: string) => void;
}): React.ReactElement {
  const label = `${field.label}${field.required ? " (needed)" : ""}`;
  const { kind } = field;
  if (kind.kind === "boolean") {
    return (
      <label className="conversation-field is-check">
        <input type="checkbox" checked={value === "true"} onChange={(e) => onChange(e.target.checked ? "true" : "false")} />
        {label}
      </label>
    );
  }
  if (kind.kind === "pick") {
    const picked = kind.options.includes(value);
    const otherOn = kind.other && value !== "" && !picked;
    return (
      <label className="conversation-field">
        <span>{label}</span>
        <select value={otherOn ? OTHER : value} onChange={(e) => onChange(e.target.value === OTHER ? " " : e.target.value)}>
          <option value="">Choose…</option>
          {kind.options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
          {kind.other ? <option value={OTHER}>Something else…</option> : null}
        </select>
        {otherOn ? <input aria-label={`${field.label}: your own answer`} value={value.trimStart()} onChange={(e) => onChange(e.target.value === "" ? " " : e.target.value)} /> : null}
      </label>
    );
  }
  return (
    <label className="conversation-field">
      <span>{label}</span>
      <input type={kind.kind === "secret" ? "password" : kind.kind === "number" ? "number" : "text"} value={value} onChange={(e) => onChange(e.target.value)} />
    </label>
  );
}

/** The pick-list entry that opens a text box for an answer of the human's own. */
const OTHER = "\u0000other";
