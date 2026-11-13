import { useCallback, useContext, useState } from "react";
import type { Project } from "../../types";
import { message } from "../../lib/errors";
import { useLoad } from "../../state/useLoad";
import { RoutingContext } from "./RoutingContext";
import type { RoutingApi } from "../../api/RoutingApi";

function Choice({ api, projectId }: { readonly api: RoutingApi; readonly projectId: string }): React.ReactElement {
  const load = useCallback(() => api.floorRouting(projectId), [api, projectId]);
  const { state, set } = useLoad(load);
  const [error, setError] = useState<string | null>(null);

  if (state.phase === "reading") {
    return (
      <p className="panel-hint" role="status">
        Reading…
      </p>
    );
  }
  if (state.phase === "failed") {
    return (
      <p className="form-error" role="alert">
        Could not read this floor’s account context: {state.message}.
      </p>
    );
  }
  const view = state.value;
  if (view.contexts.length === 0) return <p className="panel-hint">No account contexts yet. Add one in Settings, under Routing.</p>;
  const choose = (id: string): void => {
    setError(null);
    api.setFloorContext(projectId, id === "" ? null : id).then(set, (err: unknown) => setError(message(err)));
  };
  return (
    <>
      <select data-testid="floor-context" aria-label="Account context" value={view.context ?? ""} onChange={(e) => choose(e.target.value)}>
        <option value="">Not set</option>
        {view.contexts.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
      </select>
      {view.context === null ? <p className="panel-hint">Desks on this floor start as they do today.</p> : null}
      {view.enabled ? null : <p className="panel-hint">Routing is switched off in Settings, so this has no effect yet.</p>}
      {error === null ? null : (
        <p className="form-error" role="alert">
          Could not change it: {error}.
        </p>
      )}
    </>
  );
}

/** The Info panel's Account context row: which set of accounts this floor's desks may use. */
export function FloorContextRow({ project }: { readonly project: Project }): React.ReactElement | null {
  const api = useContext(RoutingContext);
  if (api === null) return null;
  return (
    <>
      <dt>Account context</dt>
      <dd>
        <Choice api={api} projectId={project.id} />
      </dd>
    </>
  );
}
