import { useCallback, useEffect, useState } from "react";
import type { ToolUsage, UsageWindow } from "../../types";
import { TOOL_LABEL } from "../../state/selectors";
import { useNow } from "../../state/useNow";
import { message } from "../../lib/errors";
import { relativeTime, resetsLabel } from "../../lib/time";

const REFRESH_MS = 60_000;

interface Props {
  readonly loadUsage: (refresh: boolean) => Promise<readonly ToolUsage[]>;
}

/** Meter colour by how much of the window is used: green, amber from 70%, red from 90%. */
function level(percent: number): "ok" | "wait" | "fault" {
  return percent >= 90 ? "fault" : percent >= 70 ? "wait" : "ok";
}

function Meter({ window, now }: { readonly window: UsageWindow; readonly now: Date }): React.ReactElement {
  const percent = Math.min(100, Math.max(0, window.percent));
  return (
    <li className="usage-window">
      <div className="usage-window-head">
        <span>{window.label}</span>
        {window.resetsAt === null ? null : <small>Resets {resetsLabel(window.resetsAt, now)}</small>}
        <b>{Math.round(percent)}%</b>
      </div>
      <div
        className="usage-meter"
        role="meter"
        aria-label={window.label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(percent)}
        data-level={level(percent)}
      >
        <span style={{ width: `${percent}%` }} />
      </div>
    </li>
  );
}

function Sign({ usage, now }: { readonly usage: ToolUsage; readonly now: Date }): React.ReactElement {
  const unavailable = usage.unavailable !== null;
  return (
    <section className={unavailable ? "usage-sign is-unavailable" : "usage-sign"} aria-label={`${TOOL_LABEL[usage.tool]} usage`} data-testid="usage-sign">
      <h2>{TOOL_LABEL[usage.tool]}</h2>
      {usage.unavailable !== null ? (
        <p className="usage-reason">{usage.unavailable}.</p>
      ) : (
        <>
          <ul>
            {usage.windows.map((w) => (
              <Meter key={w.id} window={w} now={now} />
            ))}
          </ul>
          {usage.checkedAt === null ? null : <p className="usage-asof">As of {relativeTime(usage.checkedAt, now)}</p>}
        </>
      )}
    </section>
  );
}

/** Standing signs beside the building: each installed tool's usage limits, as the tool last measured them. Re-check has Claude Code measure again. */
export function UsageSigns({ loadUsage }: Props): React.ReactElement | null {
  const [usage, setUsage] = useState<readonly ToolUsage[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const now = useNow();

  const refresh = useCallback((fresh: boolean) => {
    setBusy(true);
    return loadUsage(fresh)
      .then((next) => {
        setUsage(next);
        setError(null);
      })
      .catch((err: unknown) => setError(message(err)))
      .finally(() => setBusy(false));
  }, [loadUsage]);

  useEffect(() => {
    void refresh(false);
    const timer = setInterval(() => void refresh(false), REFRESH_MS);
    return () => clearInterval(timer);
  }, [refresh]);

  if (usage === null && error === null) return null;
  return (
    <aside className="usage-signs" aria-label="Usage limits">
      <button type="button" className="btn" disabled={busy} onClick={() => void refresh(true)}>
        {busy ? "Checking" : "Re-check"}
      </button>
      {(usage ?? []).map((u) => (
        <Sign key={u.tool} usage={u} now={now} />
      ))}
      {error === null ? null : (
        <p className="form-error" role="alert">
          Could not read usage: {error}. Try again.
        </p>
      )}
    </aside>
  );
}
