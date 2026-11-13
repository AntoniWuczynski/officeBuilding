import { useState } from "react";
import { message } from "../../lib/errors";
import { PLACE } from "../../state/placeWords";
import type { PlaceWords } from "../../state/placeWords";

interface Props {
  readonly name: string;
  /** Agents still running on the floor: they are stopped before it comes down. */
  readonly agents: number;
  readonly onDemolish: () => Promise<void>;
  readonly onCancel: () => void;
  readonly words?: PlaceWords;
}

/** Confirm pulling a floor down, and say what happens to the agents on it. */
export function DemolishDialog({ name, agents, onDemolish, onCancel, words = PLACE.office }: Props): React.ReactElement {
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
      <h2 id="demolish-title">{words.removeTitle(name)}</h2>
      <p className="panel-hint">
        {agents === 0
          ? "No agents are working here. "
          : `${agents === 1 ? "The agent" : `All ${agents} agents`} on this ${words.noun} will be stopped first, cleanly, so ${agents === 1 ? "its" : "their"} work so far is saved. `}
        {words.removeNote}
      </p>
      {error === null ? null : (
        <p className="form-error" role="alert">
          {words.removeFailed}: {error}.
        </p>
      )}
      <div className="dialog-actions">
        <button type="button" className="btn" onClick={onCancel} disabled={busy}>
          Cancel
        </button>
        <button type="submit" className="btn btn-danger" data-testid="demolish-submit" disabled={busy} autoFocus>
          {busy ? (agents === 0 ? words.removeBusy : "Stopping agents") : words.removeVerb}
        </button>
      </div>
    </form>
  );
}
