import type { Question, Session } from "../../types";
import { SESSION_STATE_SEVERITY, STATE_LABEL, TOOL_LABEL, helpersLine } from "../../state/selectors";
import { EditableSign } from "./EditableSign";
import "./place.css";

/** The panel's element id: only one is open at a time, so the nameplates can all point at it. */
export const PLACE_PANEL_ID = "place-panel";

/** A place an agent can be moved to. */
export interface MoveTarget {
  readonly id: string;
  readonly name: string;
}

interface Props {
  /** The element id the nameplate's aria-controls points at. */
  readonly id?: string;
  readonly projectId: string;
  readonly name: string;
  /** "island" or "floor": what the place is called in this view. */
  readonly noun: string;
  /** The button that goes inside: "Open island" or "Enter floor". */
  readonly enterLabel: string;
  readonly sessions: readonly Session[];
  readonly counts: { readonly active: number; readonly total: number };
  /** The place's open questions, so a waiting agent's ask shows in its row. */
  readonly asks: readonly Question[];
  readonly moveTargets: readonly MoveTarget[];
  /** The place is going away: it cannot be entered. */
  readonly leaving: boolean;
  /** Skin hooks: the class that places the panel in its view, and a ref for moving focus to the heading. */
  readonly className: string;
  readonly headingRef?: React.Ref<HTMLHeadingElement>;
  readonly onRename: (projectId: string, name: string) => void;
  readonly onMove: (sessionId: string, projectId: string) => void;
  readonly onOpenSession: (session: Session) => void;
  readonly onEnter: (projectId: string) => void;
  /** Shown as a Close button when the view lets the panel be put away. */
  readonly onClose?: () => void;
}

/** The most urgent agent first; agents equally urgent keep the place's own order. */
export function byUrgency(sessions: readonly Session[]): Session[] {
  return [...sessions].sort((a, b) => SESSION_STATE_SEVERITY[b.state] - SESSION_STATE_SEVERITY[a.state]);
}

/** One place's agents, shared by both views: rename it, open an agent (with what it waits to be asked shown), move an agent, go inside. */
export function PlacePanel({ id, projectId, name, noun, enterLabel, sessions, counts, asks, moveTargets, leaving, className, headingRef, onRename, onMove, onOpenSession, onEnter, onClose }: Props): React.ReactElement {
  const pending = asks.length;
  return (
    <aside id={id} className={`place-panel ${className}`} data-testid="place-panel" aria-label={`${name} ${noun}`}>
      <h2 ref={headingRef} tabIndex={-1}>
        <EditableSign key={projectId} name={name} onRename={(next) => onRename(projectId, next)} noun={noun} />
      </h2>
      <p>
        {counts.total} {counts.total === 1 ? "agent" : "agents"}, {counts.active} working
        {pending > 0 ? `, ${pending} waiting on you` : ""}
      </p>
      <ul>
        {byUrgency(sessions).map((s) => {
          const ask = asks.find((q) => q.sessionId === s.id);
          return (
            <li key={s.id} data-state={s.state}>
              <button type="button" className="place-row" data-testid="place-row" onClick={() => onOpenSession(s)}>
                <b>{s.title}</b>
                <span>
                  {STATE_LABEL[s.state]} · {TOOL_LABEL[s.tool]}
                  {helpersLine(s) === null ? "" : ` · ${helpersLine(s)}`}
                  {s.worktree ? " · in a worktree" : ""}
                </span>
                {ask === undefined ? null : <span className="place-ask">{ask.prompt}</span>}
              </button>
              {moveTargets.length > 0 ? (
                <select className="place-move" data-testid="place-move" aria-label={`Move ${s.title} to another ${noun}`} value={projectId} onChange={(e) => onMove(s.id, e.target.value)}>
                  <option value={projectId}>Move to...</option>
                  {moveTargets.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name}
                    </option>
                  ))}
                </select>
              ) : null}
            </li>
          );
        })}
      </ul>
      <button type="button" className="btn btn-primary place-enter" data-testid="place-enter" disabled={leaving} onClick={() => onEnter(projectId)}>
        {enterLabel}
      </button>
      {onClose === undefined ? null : (
        <button type="button" className="btn place-close" onClick={onClose}>
          Close
        </button>
      )}
    </aside>
  );
}
