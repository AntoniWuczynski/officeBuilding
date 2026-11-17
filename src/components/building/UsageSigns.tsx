import { useCallback, useEffect, useState } from "react";
import type { ToolUsage, UsageWindow } from "../../types";
import { TOOL_LABEL } from "../../state/selectors";
import { useNow } from "../../state/useNow";
import { message } from "../../lib/errors";
import { relativeTime, resetsLabel } from "../../lib/time";
import { readPref, writePref } from "../../lib/prefs";
import { everyWhileVisible } from "../../lib/visibleTimer";

const REFRESH_MS = 60_000;

interface Props {
  readonly loadUsage: (refresh: boolean) => Promise<readonly ToolUsage[]>;
  /** Start as a one-line headline that opens to the full signs. */
  readonly collapsible?: boolean;
  /** Each view keeps its own last choice under this name; it starts open or folded as `startOpen` says. */
  readonly view?: { readonly name: string; readonly startOpen: boolean };
}

/** The window nearest its limit, as one line: the number that matters when the signs are folded away. */
function headline(usage: readonly ToolUsage[]): string {
  let worst: { readonly tool: string; readonly label: string; readonly percent: number } | null = null;
  for (const u of usage) {
    for (const w of u.windows) {
      const percent = Math.round(Math.min(100, Math.max(0, w.percent)));
      if (worst === null || percent > worst.percent) worst = { tool: TOOL_LABEL[u.tool], label: w.label, percent };
    }
  }
  return worst === null ? "Usage limits" : `${worst.tool} ${worst.label} ${worst.percent}%`;
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
export function UsageSigns({ loadUsage, collapsible = false, view }: Props): React.ReactElement | null {
  // Only meaningful while `collapsible`. The same signs stay mounted when the view switches, so a choice is kept per view name.
  const [chosen, setChosen] = useState<Readonly<Record<string, boolean>>>({});
  const name = view?.name ?? "";
  const prefKey = view === undefined ? null : `office-building:usage-open:${name}`;
  const stored = prefKey === null ? null : readPref(prefKey);
  const unfolded = chosen[name] ?? (stored === null ? (view?.startOpen ?? false) : stored === "1");
  const setUnfolded = (open: boolean): void => {
    setChosen({ ...chosen, [name]: open });
    if (prefKey !== null) writePref(prefKey, open ? "1" : "0");
  };
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
    return everyWhileVisible(() => void refresh(false), REFRESH_MS);
  }, [refresh]);

  if (usage === null && error === null) return null;
  const recheck = (
    <button type="button" className="btn" disabled={busy} onClick={() => void refresh(true)}>
      {busy ? "Checking" : "Re-check"}
    </button>
  );
  if (collapsible && !unfolded) {
    return (
      <aside className="usage-signs is-folded" aria-label="Usage limits">
        <button type="button" className="btn usage-headline" aria-expanded={false} onClick={() => setUnfolded(true)}>
          {headline(usage ?? [])}
        </button>
      </aside>
    );
  }
  return (
    <aside className="usage-signs" aria-label="Usage limits">
      {collapsible ? (
        <div className="usage-bar">
          {recheck}
          <button type="button" className="btn" aria-expanded onClick={() => setUnfolded(false)}>
            Hide
          </button>
        </div>
      ) : (
        recheck
      )}
      {/* Scrolls when the window is too short for every sign; the posts stay put. */}
      <div className="usage-signs-list">
        {(usage ?? []).map((u) => (
          <Sign key={u.tool} usage={u} now={now} />
        ))}
      </div>
      {error === null ? null : (
        <p className="form-error" role="alert">
          Could not read usage: {error}. Try again.
        </p>
      )}
    </aside>
  );
}
