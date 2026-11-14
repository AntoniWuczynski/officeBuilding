import { useContext, useState } from "react";
import type { Connection, Project } from "../../types";
import { linkSummary } from "../../state/selectors";
import { message } from "../../lib/errors";
import { ConnectorsContext } from "./ConnectorsContext";
import { ConnectLinear } from "./ConnectLinear";

/** The Info panel's Linear row: connect, or the link in words with Change and Disconnect. */
export function LinearRow({ project, connection }: { readonly project: Project; readonly connection: Connection | undefined }): React.ReactElement | null {
  const actions = useContext(ConnectorsContext);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (actions === null) return null;

  const disconnect = (): void => {
    setError(null);
    actions.disconnect(project.id, "linear").catch((err: unknown) => setError(`Could not disconnect: ${message(err)}.`));
  };

  return (
    <>
      <dt>Linear</dt>
      <dd>
        {open ? (
          <ConnectLinear projectId={project.id} current={connection?.link ?? null} onDone={() => setOpen(false)} />
        ) : connection === undefined ? (
          <button type="button" className="link" data-testid="linear-connect" onClick={() => setOpen(true)}>
            Connect…
          </button>
        ) : (
          <>
            <span data-testid="linear-summary">{linkSummary(connection.link)}</span>
            <span className="linear-actions">
              <button type="button" className="link" data-testid="linear-change" onClick={() => setOpen(true)}>
                Change
              </button>
              <button type="button" className="link" data-testid="linear-disconnect" onClick={disconnect}>
                Disconnect
              </button>
            </span>
          </>
        )}
        {error === null ? null : (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
      </dd>
    </>
  );
}
