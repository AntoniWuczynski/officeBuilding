import { useContext, useState } from "react";
import type { ManagerFloor, Project, QueuedNote, Session, TodoItem } from "../../types";
import { HOLD_WORDS, NOTE_STATE_WORDS, capsLine, logLine, managerLine, plural } from "../../state/selectors";
import { relativeTime } from "../../lib/time";
import { message } from "../../lib/errors";
import { Toggle } from "../Toggle";
import { ManagerContext } from "./ManagerContext";
import type { ManagerActions } from "./ManagerContext";

const PLAN_SHOWN = 5;

interface Props {
  readonly project: Project;
  readonly manager: ManagerFloor | undefined;
  /** The manager's session, once the building has bound it to its terminal. */
  readonly session: Session | undefined;
  /** The floor's sessions, for desk titles. */
  readonly sessions: readonly Session[];
  /** The floor's agent TODOs. */
  readonly todos: readonly TodoItem[];
  readonly now: Date;
  readonly onOpenSession: (session: Session) => void;
  readonly onOpenTodos: () => void;
}

/** The manager's panel: who is hired, its plan, the notes waiting and what it has done. */
export function ManagerPanel({ project, manager, session, sessions, todos, now, onOpenSession, onOpenTodos }: Props): React.ReactElement {
  const actions = useContext(ManagerContext);
  const hired = manager?.status.kind === "hired";
  return (
    <div className="panel-body" data-testid="manager-panel">
      {manager === undefined || manager.status.kind === "absent" ? (
        <Absent projectId={project.id} actions={actions} />
      ) : (
        <Hired projectId={project.id} manager={manager} lastCallAt={hired ? manager.status.lastCallAt : null} session={session} actions={actions} now={now} onOpenSession={onOpenSession} />
      )}
      <Plan todos={todos} onOpenTodos={onOpenTodos} />
      <Waiting projectId={project.id} queue={manager?.queue ?? []} sessions={sessions} actions={actions} />
      <Log manager={manager} now={now} />
    </div>
  );
}

function Absent({ projectId, actions }: { readonly projectId: string; readonly actions: ManagerActions | null }): React.ReactElement {
  const [hiring, setHiring] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const hire = (hireFloor: ManagerActions["hire"]): void => {
    setHiring(true);
    setError(null);
    hireFloor(projectId)
      .catch((err: unknown) => setError(message(err)))
      .finally(() => setHiring(false));
  };
  return (
    <>
      <p className="panel-hint" data-testid="manager-status">
        Nobody manages this floor. A manager reads the floor&apos;s TODO list and desks, hands work to the desks and asks you when a decision is yours. It cannot run commands or edit files.
      </p>
      {actions === null ? null : (
        <>
          <button type="button" className="btn btn-primary manager-action" data-testid="manager-hire" disabled={hiring} onClick={() => hire(actions.hire)}>
            {hiring ? "Hiring" : "Hire a manager"}
          </button>
          {error === null ? null : (
            <p className="form-error" role="alert" data-testid="manager-hire-error">
              {error}
            </p>
          )}
          <p className="panel-hint">
            It is a Claude Code session on your plan. Its model and effort are in Settings.{" "}
            <button type="button" className="link" data-testid="manager-settings" onClick={actions.openSettings}>
              Open Settings
            </button>
          </p>
        </>
      )}
    </>
  );
}

interface HiredProps {
  readonly projectId: string;
  readonly manager: ManagerFloor;
  readonly lastCallAt: string | null;
  readonly session: Session | undefined;
  readonly actions: ManagerActions | null;
  readonly now: Date;
  readonly onOpenSession: (session: Session) => void;
}

function Hired({ projectId, manager, lastCallAt, session, actions, now, onOpenSession }: HiredProps): React.ReactElement {
  const [busy, setBusy] = useState(false);
  const flip = (change: (on: boolean) => Promise<void>) => (on: boolean): void => {
    setBusy(true);
    change(on).finally(() => setBusy(false));
  };
  const { caps } = manager;
  return (
    <>
      <p className="manager-line" data-testid="manager-status">
        {managerLine(manager, session)}.{" "}
        {lastCallAt === null ? "It has not used the building's tools yet. If that stays, open its terminal." : `Its tools last answered ${relativeTime(lastCallAt, now)}.`}
      </p>
      {session === undefined ? null : (
        <button type="button" className="btn manager-action" data-testid="manager-open" onClick={() => onOpenSession(session)}>
          Open its terminal
        </button>
      )}
      {actions === null ? null : (
        <>
          <div className="manager-switch" data-testid="manager-paused">
            <Toggle label="Paused" checked={manager.paused} disabled={busy} onChange={flip((on) => actions.setPaused(projectId, on))} />
            <p className="panel-hint">Paused, the manager can read the floor but nothing is sent, hired or escalated. Notes already waiting are held.</p>
          </div>
          <div className="manager-switch" data-testid="manager-hiring">
            <Toggle label="May hire desks" checked={manager.mayHire} disabled={busy} onChange={flip((on) => actions.setHiring(projectId, on))} />
            <p className="panel-hint" data-testid="manager-caps">
              {capsLine(caps)}
            </p>
            {caps.unpricedDesks === 0 ? null : (
              <p className="panel-hint" data-testid="manager-unpriced">
                {plural(caps.unpricedDesks, "hired desk has", "hired desks have")} no price data, so {caps.unpricedDesks === 1 ? "it counts" : "they count"} toward the desk cap only.
              </p>
            )}
          </div>
        </>
      )}
      <p className="panel-hint">To send the manager home, type /exit in its terminal. The building never stops an agent itself.</p>
    </>
  );
}

function Plan({ todos, onOpenTodos }: { readonly todos: readonly TodoItem[]; readonly onOpenTodos: () => void }): React.ReactElement {
  const open = todos.filter((t) => !t.done).slice(0, PLAN_SHOWN);
  return (
    <section data-testid="manager-plan">
      <h3 className="panel-subhead">Plan</h3>
      {open.length === 0 ? (
        <p className="panel-empty">Nothing on the whiteboard.</p>
      ) : (
        <ul className="manager-list">
          {open.map((t) => (
            <li key={t.id}>{t.text}</li>
          ))}
        </ul>
      )}
      <button type="button" className="link" data-testid="manager-open-todo" onClick={onOpenTodos}>
        Open TODO
      </button>
    </section>
  );
}

function Waiting({ projectId, queue, sessions, actions }: { readonly projectId: string; readonly queue: readonly QueuedNote[]; readonly sessions: readonly Session[]; readonly actions: ManagerActions | null }): React.ReactElement {
  const titleOf = (id: string): string => {
    const found = sessions.find((s) => s.id === id);
    return found === undefined ? "a desk that has left" : `“${found.title}”`;
  };
  return (
    <section data-testid="manager-queue">
      <h3 className="panel-subhead">Waiting</h3>
      {queue.length === 0 ? (
        <p className="panel-empty">No notes are waiting.</p>
      ) : (
        <ul className="manager-list">
          {queue.map((n) => (
            <li key={n.id} data-testid="manager-note">
              <b>To {titleOf(n.toSessionId)}</b>
              <span className="manager-text">{n.text}</span>
              <span className="manager-when">Waiting: {HOLD_WORDS[n.hold]}.</span>
              {actions === null ? null : (
                <button type="button" className="link" data-testid="manager-withdraw" onClick={() => actions.withdraw(projectId, n.id)}>
                  Withdraw
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Log({ manager, now }: { readonly manager: ManagerFloor | undefined; readonly now: Date }): React.ReactElement {
  const log = manager?.log ?? [];
  return (
    <section data-testid="manager-log">
      <h3 className="panel-subhead">Log</h3>
      {manager?.status.kind === "absent" && log.length > 0 ? (
        <p className="panel-hint" data-testid="manager-earlier">
          Earlier
        </p>
      ) : null}
      {log.length === 0 ? (
        <p className="panel-empty">The manager has not done anything yet.</p>
      ) : (
        <ul className="manager-list">
          {log.map((e) => (
            <li key={e.id} data-testid="manager-log-entry">
              <span className="manager-when">{relativeTime(e.at, now)}</span>
              <span className="manager-text">{logLine(e)}</span>
              {e.event.kind === "note" ? <span className="manager-when">{NOTE_STATE_WORDS[e.event.state]}</span> : null}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
