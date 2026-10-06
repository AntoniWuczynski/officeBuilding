import { useState } from "react";
import { message } from "../../lib/errors";

interface Props {
  readonly name: string;
  /** Agents still running on the floor: they are stopped before it comes down. */
  readonly agents: number;
  readonly onDemolish: () => Promise<void>;
  readonly onCancel: () => void;
}

/** Confirm pulling a floor down, and say what happens to the agents on it. */
export function DemolishDialog({ name, agents, onDemolish, onCancel }: Props): React.ReactElement {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <form
      className="dialog"
      role="dialog"
      aria-labelledby="demolish-title"
      data-testid="demolish-dialog"
      onKeyDown={(e) => {
        if (e.key === "Escape" && !busy) onCancel();
      }}
      onSubmit={(e) => {
        e.preventDefault();
        setBusy(true);
        setError(null);
        onDemolish().catch((err: unknown) => {
          setBusy(false);
          setError(message(err));
        });
      }}
    >
      <h2 id="demolish-title">Demolish {name}?</h2>
      <p className="panel-hint">
        {agents === 0
          ? "No agents are working here. "
          : `${agents === 1 ? "The agent" : `All ${agents} agents`} on this floor will be stopped first, cleanly, so ${agents === 1 ? "its" : "their"} work so far is saved. `}
        The floor comes back if an agent starts work in this folder again, or if you add it from the roof. A rebuilt floor
        keeps its sign-out sheet, so anyone who left in the last 12 hours can be resumed from there.
      </p>
      {error === null ? null : (
        <p className="form-error" role="alert">
          Could not demolish the floor: {error}.
        </p>
      )}
      <div className="dialog-actions">
        <button type="button" className="btn" onClick={onCancel} disabled={busy}>
          Cancel
        </button>
        <button type="submit" className="btn btn-danger" data-testid="demolish-submit" disabled={busy} autoFocus>
          {busy ? (agents === 0 ? "Demolishing" : "Stopping agents") : "Demolish"}
        </button>
      </div>
    </form>
  );
}
