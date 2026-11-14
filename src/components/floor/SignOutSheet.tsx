import { useState } from "react";
import type { DeskHandover, EndedSession } from "../../types";
import { TOOL_LABEL, handoverOf } from "../../state/selectors";
import { message } from "../../lib/errors";
import { OpenInEditor } from "../editor/OpenInEditor";
import { signOutLine } from "../../lib/handover";
import { relativeTime } from "../../lib/time";

interface Props {
  readonly ended: readonly EndedSession[];
  readonly handovers: readonly DeskHandover[];
  readonly now: Date;
  readonly onResume: (sessionId: string) => Promise<void>;
}

/** The floor's sign-out sheet: agents that left in the last 12 hours, each a Resume away. */
export function SignOutSheet({ ended, handovers, now, onResume }: Props): React.ReactElement {
  return (
    <div className="panel-body">
      {ended.length === 0 ? (
        <p className="panel-empty">Nobody has signed out of this floor in the last 12 hours.</p>
      ) : (
        <>
          <p className="panel-hint">
            Agents that left in the last 12 hours. Resume one and it picks up its conversation in a terminal here, in the folder it ran in.
          </p>
          <ul className="signout-list">
            {ended.map((e) => (
              <SignOutRow key={e.id} ended={e} handover={handoverOf(handovers, e.id)} now={now} onResume={onResume} />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function SignOutRow({ ended, handover, now, onResume }: { readonly ended: EndedSession; readonly handover: DeskHandover | undefined; readonly now: Date; readonly onResume: (sessionId: string) => Promise<void> }): React.ReactElement {
  const [resuming, setResuming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const resume = (): void => {
    setResuming(true);
    setError(null);
    // On success the line stays disabled until its agent is back at a desk and it leaves the sheet.
    onResume(ended.id).catch((err: unknown) => {
      setError(message(err));
      setResuming(false);
    });
  };
  const handedOver = handover === undefined ? null : signOutLine(handover.state);
  const model = ended.model === null ? "" : `, ${ended.model}`;
  return (
    <li className="signout-row" data-testid="signed-out-row">
      <span className="signout-name" data-testid="signed-out-title">
        {ended.title}
      </span>
      <span className="signout-meta">
        {TOOL_LABEL[ended.tool]}
        {model}, left {relativeTime(ended.endedAt, now)}
      </span>
      {handedOver === null ? null : (
        <span className="signout-meta" data-testid="signed-out-handover">
          {handedOver}
        </span>
      )}
      <span className="signout-resume signout-actions">
        <OpenInEditor place={{ kind: "session", sessionId: ended.id }} what={`“${ended.title}”`} opens="the folder it worked in" />
        <button type="button" className="btn" aria-label={`Resume ${ended.title}`} disabled={resuming} onClick={resume}>
          {resuming ? "Resuming" : "Resume"}
        </button>
      </span>
      {error === null ? null : (
        <p className="form-error signout-error" role="alert">
          Could not resume: {error}.
        </p>
      )}
    </li>
  );
}
