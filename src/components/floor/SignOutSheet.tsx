import { useState } from "react";
import type { DeskHandover, EndedDesk, EndedSession } from "../../types";
import { END_WORDS, TOOL_LABEL, handoverOf, lastReplyLine, modelLine } from "../../state/selectors";
import { message } from "../../lib/errors";
import { WorkerFace } from "../../scene/Worker";
import { OpenInEditor } from "../editor/OpenInEditor";
import { signOutLine } from "../../lib/handover";
import { relativeTime } from "../../lib/time";
import "../building/boards.css";

/** The end's `at` is cut to whole seconds, while a session's last activity keeps its milliseconds. */
const END_AT_TRUNCATION_MS = 1000;

interface Props {
  readonly ended: readonly EndedSession[];
  readonly handovers: readonly DeskHandover[];
  readonly now: Date;
  /** What a floor's manager recorded when it ended this session, if it did. */
  readonly endedBy: (sessionId: string) => EndedDesk | undefined;
  readonly onResume: (sessionId: string) => Promise<void>;
}

/** The floor's sign-out sheet: agents that left in the last 12 hours, each a Resume away. */
export function SignOutSheet({ ended, handovers, now, endedBy, onResume }: Props): React.ReactElement {
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
              <SignOutRow key={e.id} ended={e} handover={handoverOf(handovers, e.id)} byManager={endedBy(e.id)} now={now} onResume={onResume} />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function SignOutRow({ ended, handover, byManager, now, onResume }: { readonly ended: EndedSession; readonly handover: DeskHandover | undefined; readonly byManager: EndedDesk | undefined; readonly now: Date; readonly onResume: (sessionId: string) => Promise<void> }): React.ReactElement {
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
  // The manager's end is this sign-out only if the session did nothing after it (no resume since).
  const byManagerNow = byManager !== undefined && Date.parse(ended.endedAt) < Date.parse(byManager.at) + END_AT_TRUNCATION_MS ? byManager : undefined;
  const handedOver = handover === undefined ? null : signOutLine(handover.state);
  const modelWords = modelLine(ended);
  const model = modelWords === null ? "" : `, ${modelWords}`;
  const reply = lastReplyLine(ended.lastReply);
  return (
    <li className="signout-row" data-testid="signed-out-row">
      <span className="signout-name" data-testid="signed-out-title">
        <WorkerFace of={ended} />
        {ended.title}
      </span>
      <span className="signout-meta">
        {TOOL_LABEL[ended.tool]}
        {model}, left {relativeTime(ended.endedAt, now)}
      </span>
      {reply === null ? null : (
        <span className="signout-meta reply-line" data-testid="signed-out-reply">
          {reply}
        </span>
      )}
      {handedOver === null ? null : (
        <span className="signout-meta" data-testid="signed-out-handover">
          {handedOver}
        </span>
      )}
      {byManagerNow === undefined ? null : (
        <span className="signout-meta" data-testid="signed-out-ended">
          Ended by the floor's manager {relativeTime(byManagerNow.at, now)}: {byManagerNow.reason}.
          {byManagerNow.how === "forced" ? ` ${END_WORDS.forced}.` : ""}
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
