import { useEffect, useState } from "react";
import type { QuestionAnswer } from "../../api/DiscoveryApi";
import type { OfficeSnapshot, Question } from "../../types";
import { isSessionAsk, needsYouQueue, sessionById } from "../../state/selectors";
import { useNow } from "../../state/useNow";
import { relativeTime } from "../../lib/time";
import { message } from "../../lib/errors";
import "./needs-you.css";

interface Props {
  readonly snapshot: OfficeSnapshot;
  /** Go to what is asking: its session's terminal, or the floor whose decisions file holds the call. */
  readonly onOpen: (question: Question) => void;
  readonly onAnswer: (questionId: string, answer: QuestionAnswer) => Promise<void>;
}

/** "orders ingest fix, asked 2 h ago" or "From FOUNDER_DECISIONS.md, asked 2 h ago". */
function whoAsked(snapshot: OfficeSnapshot, q: Question, now: Date): string {
  const place = snapshot.projects.find((p) => p.id === q.projectId)?.name ?? "";
  const who = q.sourceFile !== null ? `From ${q.sourceFile}` : (sessionById(snapshot, q.sessionId)?.title ?? "Unknown agent");
  return `${place === "" ? "" : `${place}: `}${who}${q.askedAt === "" ? "" : `, asked ${relativeTime(q.askedAt, now)}`}`;
}

function Row({ snapshot, q, now, onOpen, onAnswer }: { readonly snapshot: OfficeSnapshot; readonly q: Question; readonly now: Date } & Pick<Props, "onOpen" | "onAnswer">): React.ReactElement {
  const [answering, setAnswering] = useState(false);
  const [other, setOther] = useState("");
  const [error, setError] = useState<string | null>(null);
  // A hand-started session's terminal is the only place it can be answered.
  const external = q.answerVia === "terminal";
  const send = (a: QuestionAnswer): void => {
    setError(null);
    onAnswer(q.id, a).catch((err: unknown) => setError(message(err)));
  };
  return (
    <li className="needs-you-row" data-testid="needs-you-row" data-question={q.id} data-kind={isSessionAsk(q) ? "session" : "file"}>
      <p className="needs-you-prompt">{q.prompt}</p>
      <p className="needs-you-meta">{whoAsked(snapshot, q, now)}</p>
      <div className="needs-you-actions">
        <button type="button" className="btn" onClick={() => onOpen(q)}>
          Open
        </button>
        <button type="button" className="btn" aria-expanded={answering} disabled={external} title={external ? "This agent runs outside the app: answer it in its own terminal." : undefined} onClick={() => setAnswering(!answering)}>
          Answer
        </button>
      </div>
      {answering ? (
        <div className="needs-you-answer">
          {q.options.map((o, i) => (
            <button key={o.id} type="button" className="btn" onClick={() => send({ kind: "option", optionId: o.id })}>
              {String.fromCharCode(65 + i)}: {o.label}
            </button>
          ))}
          {q.allowOther ? (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                if (other.trim() !== "") send({ kind: "other", text: other });
              }}
            >
              <input aria-label="Your own answer" value={other} placeholder="Type your own answer" onChange={(e) => setOther(e.target.value)} />
              <button type="submit" className="btn" disabled={other.trim() === ""}>
                Send
              </button>
            </form>
          ) : null}
        </div>
      ) : null}
      {error === null ? null : (
        <p className="form-error" role="alert">
          Answer not sent: {error}. Try again.
        </p>
      )}
    </li>
  );
}

/** A top-bar button, in both views, opening what is waiting on you across the whole map: agents' asks first, then repo decisions-file calls. */
export function NeedsYou({ snapshot, onOpen, onAnswer }: Props): React.ReactElement {
  const [open, setOpen] = useState(false);
  const now = useNow();
  const queue = needsYouQueue(snapshot);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);
  return (
    <span className="needs-you-menu">
      <button type="button" className="btn" data-testid="needs-you" aria-expanded={open} aria-controls="needs-you-list" onClick={() => setOpen(!open)}>
        Needs you{queue.length > 0 ? ` ${queue.length}` : ""}
      </button>
      {open ? (
        <section id="needs-you-list" className="needs-you-list" aria-label="Waiting on you">
          {queue.length === 0 ? (
            <p className="needs-you-meta">Nothing is waiting on you.</p>
          ) : (
            <ol>
              {queue.map((q) => (
                <Row
                  key={q.id}
                  snapshot={snapshot}
                  q={q}
                  now={now}
                  onOpen={(question) => {
                    setOpen(false);
                    onOpen(question);
                  }}
                  onAnswer={(id, a) => onAnswer(id, a)}
                />
              ))}
            </ol>
          )}
        </section>
      ) : null}
    </span>
  );
}
