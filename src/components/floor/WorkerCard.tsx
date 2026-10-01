import { createPortal } from "react-dom";
import type { Session } from "../../types";
import { STATE_LABEL, TOOL_LABEL, modelLine } from "../../state/selectors";
import { relativeTime } from "../../lib/time";

interface Props {
  readonly session: Session;
  /** Screen point just above the worker's head (viewport pixels). */
  readonly x: number;
  readonly y: number;
  readonly now: Date;
}

/**
 * The hover card for a worker. Drawn outside the 3D scene, in a flat overlay:
 * inside the scene it is a plane at the worker's depth, and walls behind a
 * corner desk slice straight through it (WebKit sorts 3D planes strictly).
 */
export function WorkerCard({ session, x, y, now }: Props): React.ReactElement {
  const model = modelLine(session);
  return createPortal(
    <div className="worker-card" data-testid="worker-card" role="tooltip" style={{ left: x, top: y }}>
      <b>{session.title}</b>
      <span>{TOOL_LABEL[session.tool]}</span>
      {model === null ? null : <span>{model}</span>}
      <span>
        {STATE_LABEL[session.state]}, active {relativeTime(session.lastActivityAt, now)}
      </span>
    </div>,
    document.body,
  );
}
