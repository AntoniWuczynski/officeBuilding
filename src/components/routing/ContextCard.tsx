import { useState } from "react";
import type { AccountContext, ConnectionKind, RoutingSettings } from "../../types";
import { KINDS, addConnection, cleanName, kindOffer, newId, removeContext, renameContext } from "../../lib/routing";
import { CommitField } from "../settings/CommitField";
import { ConfirmRow, ConnectionCard, NAME_ERROR } from "./ConnectionCard";

interface Props {
  readonly context: AccountContext;
  readonly settings: RoutingSettings;
  readonly onChange: (next: RoutingSettings) => void;
}

/** One account context: its name, its connections in the order they are tried, and a row to add one. */
export function ContextCard({ context, settings, onChange }: Props): React.ReactElement {
  const [removing, setRemoving] = useState(false);
  const [kind, setKind] = useState<ConnectionKind>("anthropic-api-key");
  const owned = settings.connections.filter((c) => c.context === context.id);
  const connections = context.chain.flatMap((id) => owned.filter((c) => c.id === id));
  const refusal = kindOffer(settings, kind);

  const add = (): void => {
    onChange(addConnection(settings, newId("con", settings), context.id, kind));
  };

  return (
    <div className="routing-card" data-testid={`routing-context-${context.id}`}>
      <CommitField
        label="Name"
        testId={`routing-context-name-${context.id}`}
        value={context.name}
        onCommit={(text) => {
          const name = cleanName(text);
          if (name === null) return NAME_ERROR;
          onChange(renameContext(settings, context.id, name));
          return null;
        }}
      />
      <div className="routing-buttons">
        <button type="button" className="btn" onClick={() => setRemoving(true)}>
          Remove context
        </button>
      </div>
      {removing ? (
        <ConfirmRow
          question={
            owned.length === 0
              ? `Remove ${context.name}?`
              : `Remove ${context.name} and its ${owned.length} ${owned.length === 1 ? "connection" : "connections"}? Their saved keys are removed too.`
          }
          confirm="Remove"
          onConfirm={() => onChange(removeContext(settings, context.id))}
          onKeep={() => setRemoving(false)}
        />
      ) : null}
      {connections.length === 0 ? (
        <p className="panel-hint">No connections yet.</p>
      ) : (
        <>
          <ol className="routing-list">
            {connections.map((connection, index) => (
              <li key={connection.id}>
                <ConnectionCard connection={connection} position={index} last={index === connections.length - 1} settings={settings} onChange={onChange} />
              </li>
            ))}
          </ol>
          <p className="panel-hint">Tried from the top. The next one is used only when the one above reports that its allowance is used up.</p>
        </>
      )}
      <div className="routing-add">
        <label className="field">
          <span>Kind</span>
          <select value={kind} onChange={(e) => setKind(KINDS.find((k) => k.kind === e.target.value)?.kind ?? kind)}>
            {KINDS.map((k) => (
              <option key={k.kind} value={k.kind} disabled={kindOffer(settings, k.kind) !== null}>
                {k.label}
              </option>
            ))}
          </select>
        </label>
        <button type="button" className="btn" disabled={refusal !== null} onClick={add}>
          Add connection
        </button>
      </div>
      {refusal === null ? null : <p className="panel-hint">{refusal}</p>}
    </div>
  );
}
