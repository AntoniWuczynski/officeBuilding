import { useState } from "react";

interface Props {
  readonly canPickFolder: boolean;
  readonly onPickFolder: () => Promise<string | null>;
  readonly onAdd: (path: string) => Promise<void>;
  readonly onCancel: () => void;
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Add a floor: the repo folder a project lives in. */
export function AddFloorDialog({ canPickFolder, onPickFolder, onAdd, onCancel }: Props): React.ReactElement {
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <form
      className="dialog"
      role="dialog"
      aria-labelledby="add-floor-title"
      data-testid="add-floor-dialog"
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      onSubmit={(e) => {
        e.preventDefault();
        setBusy(true);
        setError(null);
        onAdd(path).catch((err: unknown) => {
          setBusy(false);
          setError(message(err));
        });
      }}
    >
      <h2 id="add-floor-title">Add a floor</h2>
      <p className="panel-hint">
        A floor is a project folder. Agents that start work there take a desk on it; until then its lights stay off.
      </p>
      <label className="field">
        <span>Folder</span>
        <div className="other-row">
          <input
            value={path}
            disabled={busy}
            placeholder="~/code/app"
            onChange={(e) => setPath(e.target.value)}
            autoFocus
          />
          {canPickFolder ? (
            <button
              type="button"
              className="btn"
              data-testid="pick-folder"
              disabled={busy}
              onClick={() => {
                setError(null);
                onPickFolder()
                  .then((picked) => {
                    if (picked !== null) setPath(picked);
                  })
                  .catch((err: unknown) => setError(message(err)));
              }}
            >
              Choose folder
            </button>
          ) : null}
        </div>
      </label>
      {error === null ? null : (
        <p className="form-error" role="alert">
          Could not add the floor: {error}.
        </p>
      )}
      <div className="dialog-actions">
        <button type="button" className="btn" onClick={onCancel} disabled={busy}>
          Cancel
        </button>
        <button type="submit" className="btn btn-primary" data-testid="add-floor-submit" disabled={busy || path.trim() === ""}>
          {busy ? "Adding" : "Add floor"}
        </button>
      </div>
    </form>
  );
}
