import { useContext, useEffect, useState } from "react";
import type { DeskRoute } from "../../types";
import { message } from "../../lib/errors";
import {
  canRetry,
  canSwitch,
  clock,
  entryStateSentence,
  problemSentence,
  reasoningSentence,
  routeHeadline,
  scopeSentence,
  servedSentence,
  switchSentence,
} from "../../lib/route";
import { KIND_LABEL } from "../../lib/routing";
import { RoutingContext } from "./RoutingContext";

const POLL_MS = 3000;

/** A routed desk's route: one line in the panel, the whole order and its allowances on request. */
export function RouteStrip({ sessionId }: { readonly sessionId: string }): React.ReactElement | null {
  const api = useContext(RoutingContext);
  const [route, setRoute] = useState<DeskRoute | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (api === null) return undefined;
    let live = true;
    let pending = false;
    const read = (): void => {
      // A read that has not come back (a Keychain prompt left open) is not stacked on.
      if (pending) return;
      pending = true;
      api.deskRoute(sessionId).then(
        (next) => {
          pending = false;
          if (!live) return;
          setRoute(next);
          setReadError(null);
        },
        (e: unknown) => {
          pending = false;
          if (live) setReadError(message(e));
        },
      );
    };
    read();
    const timer = setInterval(read, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [api, sessionId]);

  if (api === null || route === null) return null;

  const act = (call: (sessionId: string, connectionId: string) => Promise<DeskRoute>, connection: string): void => {
    setBusy(true);
    setActionError(null);
    call(sessionId, connection).then(
      (next) => {
        setRoute(next);
        setBusy(false);
      },
      (e: unknown) => {
        setActionError(message(e));
        setBusy(false);
      },
    );
  };

  const { status } = route;
  const headline = routeHeadline(route);
  const now = Math.floor(Date.now() / 1000);
  const notes = [servedSentence(status), switchSentence(status), reasoningSentence(route.reasoning)];

  return (
    <>
      <div className="route-strip" data-testid="route-strip">
        <span className="route-lamp" data-tone={headline.tone} aria-hidden="true" />
        <span>Route: {headline.text}</span>
        {readError === null ? null : (
          <span role="alert" className="route-error">
            Could not read the route: {readError}.
          </span>
        )}
        <button type="button" className="btn dark-btn" aria-expanded={open} data-testid="route-details-toggle" onClick={() => setOpen(!open)}>
          Details
        </button>
      </div>
      {open ? (
        <div className="route-details" data-testid="route-details">
          {status.blockedSince === null ? null : (
            <p role="status">
              Blocked since {clock(status.blockedSince)}. No connection in this desk's order can take its requests, and nothing is sent until you retry one or switch. Then tell the agent to carry on: it
              stopped when it was refused.
            </p>
          )}
          {route.requests === 0 ? <p>No request from this desk has reached the building yet.</p> : null}
          {notes.map((note) => (note === null ? null : <p key={note}>{note}</p>))}
          <ol className="routing-list route-entries">
            {status.entries.map((entry, index) => (
              <li key={entry.connection}>
                <b>
                  {index + 1}. {entry.name}, {KIND_LABEL[entry.kind]}
                </b>{" "}
                {entryStateSentence(entry)}
                {entry.exhausted.map((scope) => (
                  <p key={`${scope.bucket.kind}-${scope.observedAt}-${scopeSentence(scope, now)}`}>{scopeSentence(scope, now)}</p>
                ))}
                {entry.problem === null ? null : (
                  <p>
                    Last problem at {clock(entry.problem.at)}: {problemSentence(entry.problem.kind)}.
                  </p>
                )}
                <span className="route-buttons">
                  {canRetry(entry) ? (
                    <button type="button" className="btn dark-btn" disabled={busy} data-testid={`route-retry-${entry.connection}`} onClick={() => act(api.retryRoute.bind(api), entry.connection)}>
                      Retry
                    </button>
                  ) : null}
                  {canSwitch(entry) ? (
                    <button type="button" className="btn dark-btn" disabled={busy} data-testid={`route-switch-${entry.connection}`} onClick={() => act(api.switchRoute.bind(api), entry.connection)}>
                      Switch to this
                    </button>
                  ) : null}
                </span>
              </li>
            ))}
          </ol>
          {actionError === null ? null : <p role="alert">Could not do that: {actionError}.</p>}
          {route.recordsProblem === null ? null : <p>{route.recordsProblem}</p>}
          <p className="route-warning">
            This desk's requests go through the building. If Office Building quits while the desk is running, the desk loses its model access. Start it again from the sign-out sheet.
          </p>
        </div>
      ) : null}
    </>
  );
}
