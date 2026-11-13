import { createPortal } from "react-dom";
import type { DeskHandoverState, QueuedNote, Session } from "../../types";
import { HOLD_WORDS, STATE_LABEL, TOOL_LABEL, helpersLine, modelLine, plural } from "../../state/selectors";
import { cardLine } from "../../lib/handover";
import { relativeTime } from "../../lib/time";

interface Props {
  readonly session: Session;
  /** Screen point just above the worker's head (viewport pixels). */
  readonly x: number;
  readonly y: number;
  readonly now: Date;
  /** The last line: how to open its code, or null. */
  readonly editorHint: string | null;
  /** The floor's manager drives this desk. */
  readonly managed: boolean;
  /** Notes from the manager still waiting for this desk. */
  readonly waiting: readonly QueuedNote[];
  /** What this desk's handover has to say, if anything. */
  readonly handover: DeskHandoverState | undefined;
}

/**
 * The hover card for a worker. Drawn outside the 3D scene, in a flat overlay:
 * inside the scene it is a plane at the worker's depth, and walls behind a
 * corner desk slice straight through it (WebKit sorts 3D planes strictly).
 */
export function WorkerCard({ session, x, y, now, editorHint, managed, waiting, handover }: Props): React.ReactElement {
  const model = modelLine(session);
  const helpers = helpersLine(session);
  const handedOver = handover === undefined ? null : cardLine(handover, now);
  return createPortal(
    <div className="worker-card" data-testid="worker-card" role="tooltip" style={{ left: x, top: y }}>
      <b>{session.title}</b>
      <span>{TOOL_LABEL[session.tool]}</span>
      {model === null ? null : <span>{model}</span>}
      <span>
        {STATE_LABEL[session.state]}, active {relativeTime(session.lastActivityAt, now)}
      </span>
      {helpers === null ? null : <span>{helpers}</span>}
      {session.worktree ? <span>{session.branch === null ? "Worktree" : `Worktree on ${session.branch}`}</span> : null}
      {managed ? <span>Managed by the floor&apos;s manager</span> : null}
      {waiting[0] === undefined ? null : (
        <span>
          {plural(waiting.length, "note", "notes")} from the manager waiting: {HOLD_WORDS[waiting[0].hold]}
        </span>
      )}
      {handedOver === null ? null : <span>{handedOver}</span>}
      {editorHint === null ? null : <span>{editorHint}</span>}
    </div>,
    document.body,
  );
}
