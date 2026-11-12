import { useContext, useState } from "react";
import type { Project, QaFloor, Session } from "../../types";
import { qaLine, reviewLine } from "../../state/selectors";
import { message } from "../../lib/errors";
import { relativeTime } from "../../lib/time";
import { Toggle } from "../Toggle";
import { QaContext } from "./QaContext";

interface Props {
  readonly project: Project;
  readonly qa: QaFloor | undefined;
  readonly sessions: readonly Session[];
  readonly now: Date;
  readonly onOpenSession: (session: Session) => void;
}

/** The Info panel's Quality assurance row: the switch, what the desk is doing, and its last reviews. */
export function QaRow({ project, qa, sessions, now, onOpenSession }: Props): React.ReactElement | null {
  const actions = useContext(QaContext);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (actions === null || qa === undefined) return null;

  const switchTo = (enabled: boolean): void => {
    setError(null);
    setConfirming(false);
    setBusy(true);
    actions
      .setFloor(project.id, enabled)
      .catch((err: unknown) => setError(`Could not switch QA: ${message(err)}.`))
      .finally(() => setBusy(false));
  };
  const desk = sessions.find((s) => s.id === qa.run?.sessionId);

  return (
    <>
      <dt>Quality assurance</dt>
      <dd>
        <div data-testid="qa-toggle">
          <Toggle
            label="Review my pull requests"
            checked={qa.enabled || confirming}
            disabled={busy}
            onChange={(on) => (on ? setConfirming(true) : qa.enabled ? switchTo(false) : setConfirming(false))}
          />
        </div>
        {confirming ? (
          <div className="qa-confirm" data-testid="qa-confirm">
            <p>Reviews of your pull requests on this floor will be posted on GitHub from your account, signed Office Building Quality Assurance. The floor will be pinned.</p>
            <span className="qa-actions">
              <button type="button" className="btn" data-testid="qa-confirm-yes" onClick={() => switchTo(true)}>
                Turn on
              </button>
              <button type="button" className="link" onClick={() => setConfirming(false)}>
                Cancel
              </button>
            </span>
          </div>
        ) : null}
        {error === null ? null : (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        {qa.enabled ? (
          <>
            <p className="qa-status">
              <span data-testid="qa-status" role="status">
                {qaLine(qa, now)}
              </span>
              {desk === undefined ? null : (
                <button type="button" className="link" data-testid="qa-open-desk" onClick={() => onOpenSession(desk)}>
                  Open its desk
                </button>
              )}
            </p>
            <span className="qa-actions">
              <button type="button" className="link" data-testid="qa-recheck" onClick={actions.recheck}>
                Check now
              </button>
              <button type="button" className="link" onClick={actions.openSettings}>
                Settings
              </button>
            </span>
          </>
        ) : null}
        {qa.reviews.length === 0 ? null : (
          <ul className="qa-reviews">
            {qa.reviews.map((r) => (
              <li key={`${r.number}@${r.head}`}>
                <button
                  type="button"
                  className="link"
                  data-testid="qa-review"
                  aria-label={`Open the review of pull request ${r.number}, ${r.title}, in the browser`}
                  onClick={() => actions.openReview(project.id, r)}
                >
                  #{r.number} {r.title}
                </button>
                <span>
                  {reviewLine(r)}, {relativeTime(r.at, now)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </dd>
    </>
  );
}
