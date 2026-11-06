import { useState } from "react";
import type { Shell } from "../../types";
import { message } from "../../lib/errors";
import { relativeTime } from "../../lib/time";

interface Props {
  readonly shells: readonly Shell[];
  readonly now: Date;
  readonly onOpenShell: (shellId: string) => void;
  readonly onNewShell: () => Promise<void>;
  readonly onEndShell: (shellId: string) => Promise<void>;
}

/** The manager's own terminals on this floor: plain shells, each an Open away. */
export function ShellsPanel({ shells, now, onOpenShell, onNewShell, onEndShell }: Props): React.ReactElement {
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const open = (): void => {
    setOpening(true);
    setError(null);
    onNewShell()
      .catch((err: unknown) => setError(`Could not open a shell: ${message(err)}`))
      .finally(() => setOpening(false));
  };
  const end = (shell: Shell): void => {
    setError(null);
    onEndShell(shell.id).catch((err: unknown) => setError(`Could not end Shell ${shell.number}: ${message(err)}`));
  };
  return (
    <div className="panel-body">
      <p className="panel-hint">Plain shells in this floor&apos;s folder, for a quick command. Cmd+N opens a new one. Closing the panel keeps a shell running. End, or typing exit, ends it.</p>
      <button type="button" className="btn btn-primary" data-testid="new-shell" disabled={opening} onClick={open}>
        {opening ? "Opening" : "New shell"}
      </button>
      {error === null ? null : (
        <p className="signout-error" role="alert">
          {error}.
        </p>
      )}
      {shells.length === 0 ? (
        <p className="panel-empty">No shells open on this floor.</p>
      ) : (
        <ul className="signout-list">
          {shells.map((s) => (
            <li key={s.id} className="signout-row" data-testid="shell-row">
              <span className="signout-name">Shell {s.number}</span>
              <span className="signout-meta">Opened {relativeTime(s.startedAt, now)}</span>
              <span className="signout-resume shell-actions">
                <button type="button" className="btn" onClick={() => onOpenShell(s.id)}>
                  Open
                </button>
                <button type="button" className="btn" aria-label={`End Shell ${s.number}`} onClick={() => end(s)}>
                  End
                </button>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
