import { useState } from "react";
import type { Decision, OfficeSnapshot, Question, Session } from "../../types";
import type { QuestionAnswer } from "../../api/DiscoveryApi";
import { TOOL_LABEL, sessionById } from "../../state/selectors";
import { relativeTime } from "../../lib/time";

interface Props {
  readonly snapshot: OfficeSnapshot;
  readonly questions: readonly Question[];
  readonly decisions: readonly Decision[];
  readonly now: Date;
  readonly onAnswer: (questionId: string, answer: QuestionAnswer) => Promise<void>;
  readonly onOpenSession: (session: Session) => void;
}

/** "orders ingest fix, asked 2 h ago" or "From FOUNDER_DECISIONS.md". */
function askedBy(q: Question, asker: Session | undefined, now: Date): string {
  const when = q.askedAt === "" ? "" : `, asked ${relativeTime(q.askedAt, now)}`;
  if (q.sourceFile !== null) return `From ${q.sourceFile}${when}`;
  return `${asker === undefined ? "Unknown agent" : asker.title}${when}`;
}

/** Decisions → questions ordered by priority → one question with A / B / Other (the sketch). */
export function DecisionsPanel({ snapshot, questions, decisions, now, onAnswer, onOpenSession }: Props): React.ReactElement {
  const [openId, setOpenId] = useState<string | null>(null);
  const open = openId === null ? undefined : questions.find((q) => q.id === openId);

  if (open !== undefined) {
    return (
      <QuestionDetail snapshot={snapshot} question={open} now={now} onBack={() => setOpenId(null)} onAnswer={onAnswer} onOpenSession={onOpenSession} />
    );
  }

  return (
    <div className="panel-body">
      <h3 className="panel-subhead">Waiting on you</h3>
      {questions.length === 0 ? (
        <p className="panel-empty" data-testid="queue-empty">
          Nothing is waiting on you. Agents that need a decision appear here, most urgent first.
        </p>
      ) : (
        <ol className="question-list">
          {questions.map((q, i) => {
            const asker = sessionById(snapshot, q.sessionId);
            return (
              <li key={q.id}>
                <button type="button" className="question-row" data-testid="question-row" data-question={q.id} onClick={() => setOpenId(q.id)}>
                  <span className="question-rank" aria-label={`Priority ${i + 1}`}>{i + 1}</span>
                  <span className="question-text">
                    <span className="question-prompt">{q.prompt}</span>
                    <span className="question-meta">{askedBy(q, asker, now)}</span>
                  </span>
                </button>
              </li>
            );
          })}
        </ol>
      )}

      <h3 className="panel-subhead">Decided</h3>
      {decisions.length === 0 ? (
        <p className="panel-empty">No decisions recorded on this floor yet.</p>
      ) : (
        <ul className="decision-list">
          {decisions.map((d) => (
            <li key={d.id}>
              <b>{d.title}</b>
              <span className="decision-detail">{d.detail}</span>
              <span className="decision-meta">
                {d.by === "human" ? "You" : "An agent"} decided{d.decidedAt === "" ? "" : ` ${relativeTime(d.decidedAt, now)}`}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface DetailProps {
  readonly snapshot: OfficeSnapshot;
  readonly question: Question;
  readonly now: Date;
  readonly onBack: () => void;
  readonly onAnswer: (questionId: string, answer: QuestionAnswer) => Promise<void>;
  readonly onOpenSession: (session: Session) => void;
}

function QuestionDetail({ snapshot, question, now, onBack, onAnswer, onOpenSession }: DetailProps): React.ReactElement {
  const [other, setOther] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const asker = sessionById(snapshot, question.sessionId);

  const send = (answer: QuestionAnswer): void => {
    setSending(true);
    setError(null);
    onAnswer(question.id, answer)
      .then(onBack)
      .catch((err: unknown) => {
        setSending(false);
        setError(err instanceof Error ? err.message : String(err));
      });
  };

  return (
    <div className="panel-body" data-testid="question-detail">
      <button type="button" className="link back-link" onClick={onBack}>
        All decisions
      </button>
      <p className="question-full">{question.prompt}</p>
      <p className="question-meta">
        {asker === undefined ? askedBy(question, asker, now) : `${asker.title} (${TOOL_LABEL[asker.tool]}), asked ${relativeTime(question.askedAt, now)}`}
      </p>
      {question.context === "" ? null : <p className="question-context">{question.context}</p>}
      {question.sourceFile === null || question.answerVia !== "file" ? null : (
        <p className="panel-hint">Your answer is written into {question.sourceFile}, where the agents read it.</p>
      )}
      {question.answerVia === "terminal" ? (
        <>
          <p className="panel-hint">
            {asker?.control === "full"
              ? "This agent runs in the app, so answer it in its terminal."
              : "This agent was started outside the app, so answer it in its own terminal."}
          </p>
          <ol className="options options-readonly">
            {question.options.map((o, i) => (
              <li key={o.id} className="option">
                <span className="option-key">{String.fromCharCode(65 + i)}</span>
                <span>{o.label}</span>
              </li>
            ))}
          </ol>
          {asker === undefined ? null : (
            <button type="button" className="btn btn-primary" data-testid="question-raise" onClick={() => onOpenSession(asker)}>
              {asker.control === "full" ? "Open its terminal" : "Bring its terminal to the front"}
            </button>
          )}
        </>
      ) : (
        <AppAnswer question={question} sending={sending} other={other} setOther={setOther} send={send} />
      )}
      {error === null ? null : (
        <p className="form-error" role="alert">
          Answer not sent: {error}. Try again.
        </p>
      )}
    </div>
  );
}

interface AppAnswerProps {
  readonly question: Question;
  readonly sending: boolean;
  readonly other: string;
  readonly setOther: (v: string) => void;
  readonly send: (answer: QuestionAnswer) => void;
}

/** A / B / Other for sessions the app can deliver answers to. */
function AppAnswer({ question, sending, other, setOther, send }: AppAnswerProps): React.ReactElement {
  return (
    <>
      <div className="options">
        {question.options.map((o, i) => (
          <button
            key={o.id}
            type="button"
            className="option"
            data-testid="question-option"
            disabled={sending}
            onClick={() => send({ kind: "option", optionId: o.id })}
          >
            <span className="option-key">{String.fromCharCode(65 + i)}</span>
            <span>{o.label}</span>
          </button>
        ))}
      </div>
      {question.allowOther ? (
        <form
          className="other"
          onSubmit={(e) => {
            e.preventDefault();
            if (other.trim() !== "") send({ kind: "other", text: other });
          }}
        >
          <label htmlFor={`other-${question.id}`}>Other</label>
          <div className="other-row">
            <input
              id={`other-${question.id}`}
              value={other}
              disabled={sending}
              placeholder="Type your own answer"
              onChange={(e) => setOther(e.target.value)}
            />
            <button type="submit" className="btn" data-testid="question-other-submit" disabled={sending || other.trim() === ""}>
              {sending ? "Sending" : "Send"}
            </button>
          </div>
        </form>
      ) : null}
    </>
  );
}
