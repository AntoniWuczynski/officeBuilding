import { useCallback, useContext, useState } from "react";
import type { RoutingConnection, RoutingKeyStatus, RoutingSettings, TierId } from "../../types";
import {
  KIND_LABEL,
  TIERS,
  formatEfforts,
  kindTraits,
  moveConnection,
  patchConnection,
  cleanName,
  NAME_MAX,
  parseEfforts,
  removeConnection,
  setTarget,
} from "../../lib/routing";
import { CommitField } from "../settings/CommitField";
import type { RoutingApi } from "../../api/RoutingApi";
import { KeyField } from "../settings/KeyField";
import { Toggle } from "../Toggle";
import { RoutingContext } from "./RoutingContext";

/** A question inside a card, answered in place. */
export function ConfirmRow({ question, confirm, onConfirm, onKeep }: { readonly question: string; readonly confirm: string; readonly onConfirm: () => void; readonly onKeep: () => void }): React.ReactElement {
  return (
    <div className="routing-confirm" role="group" aria-label={question}>
      <p className="panel-hint">{question}</p>
      <div className="routing-buttons">
        <button type="button" className="btn" onClick={onConfirm}>
          {confirm}
        </button>
        <button type="button" className="btn" onClick={onKeep}>
          Keep
        </button>
      </div>
    </div>
  );
}

export const NAME_ERROR = `Enter a name of 1 to ${NAME_MAX} characters.`;

function keySentence(status: RoutingKeyStatus): string {
  switch (status.kind) {
    case "missing":
      return "No API key is saved for this connection.";
    case "saved":
      return "An API key is saved in your Keychain. It has not been checked with the provider.";
    case "unavailable":
      return `The Keychain did not hand over the key: ${status.reason}.`;
  }
}

function ApiKey({ id }: { readonly id: string }): React.ReactElement | null {
  const routing = useContext(RoutingContext);
  return routing === null ? null : <KeyedApiKey id={id} routing={routing} />;
}

function KeyedApiKey({ id, routing }: { readonly id: string; readonly routing: RoutingApi }): React.ReactElement {
  const load = useCallback(() => routing.keyStatus(id), [routing, id]);
  return (
    <KeyField
      load={load}
      save={(key) => routing.setKey(id, key)}
      sentence={keySentence}
      removable={(status) => status.kind === "saved"}
      busy="Saving the key…"
      placeholder="Paste an API key"
      testId={`routing-key-${id}`}
      hint="The key is kept in the macOS Keychain and nowhere else."
    />
  );
}

const EFFORT_ERROR = "Write each pair as asked=sent, separated by commas.";

function TierRow({ connection, tier, label, translated, settings, onChange }: {
  readonly connection: RoutingConnection;
  readonly tier: TierId;
  readonly label: string;
  readonly translated: boolean;
  readonly settings: RoutingSettings;
  readonly onChange: (next: RoutingSettings) => void;
}): React.ReactElement {
  const target = connection.targets[tier];
  const model = target?.model ?? "";
  const efforts = target?.efforts ?? {};
  return (
    <div className="routing-tier">
      <strong>{label}</strong>
      <CommitField
        label="Model name on this connection"
        testId={`routing-model-${connection.id}-${tier}`}
        value={model}
        onCommit={(text) => {
          onChange(setTarget(settings, connection.id, tier, text, efforts));
          return null;
        }}
      />
      <CommitField
        label={translated ? "Efforts, asked to sent" : "Efforts that change, asked to sent"}
        testId={`routing-efforts-${connection.id}-${tier}`}
        value={formatEfforts(efforts)}
        placeholder="xhigh=high, max=high"
        disabled={model === ""}
        onCommit={(text) => {
          const parsed = parseEfforts(text);
          if (parsed === null) return EFFORT_ERROR;
          onChange(setTarget(settings, connection.id, tier, model, parsed));
          return null;
        }}
      />
    </div>
  );
}

interface Props {
  readonly connection: RoutingConnection;
  readonly position: number;
  readonly last: boolean;
  readonly settings: RoutingSettings;
  readonly onChange: (next: RoutingSettings) => void;
}

/** One way a desk's requests can be served, with its opt-ins, its key and its model names. */
export function ConnectionCard({ connection, position, last, settings, onChange }: Props): React.ReactElement {
  const [removing, setRemoving] = useState(false);
  const traits = kindTraits(connection.kind);
  const id = connection.id;
  return (
    <details className="routing-card routing-connection">
      <summary>
        {position + 1}. {connection.name}, {KIND_LABEL[connection.kind]}
      </summary>
      <CommitField
        label="Name"
        testId={`routing-name-${id}`}
        value={connection.name}
        onCommit={(text) => {
          const name = cleanName(text);
          if (name === null) return NAME_ERROR;
          onChange(patchConnection(settings, id, { name }));
          return null;
        }}
      />
      <div className="routing-buttons">
        <button type="button" className="btn" disabled={position === 0} onClick={() => onChange(moveConnection(settings, id, -1))}>
          Move up
        </button>
        <button type="button" className="btn" disabled={last} onClick={() => onChange(moveConnection(settings, id, 1))}>
          Move down
        </button>
        <button type="button" className="btn" onClick={() => setRemoving(true)}>
          Remove connection
        </button>
      </div>
      {removing ? (
        <ConfirmRow
          question={traits.holdsKey ? "Remove this connection? Its saved key is removed too." : "Remove this connection?"}
          confirm="Remove"
          onConfirm={() => onChange(removeConnection(settings, id))}
          onKeep={() => setRemoving(false)}
        />
      ) : null}
      {connection.kind === "claude-subscription" ? (
        <p className="panel-hint">Requests go to Anthropic untouched, on the desk’s own Claude login. The building never stores that login and never sends it anywhere else.</p>
      ) : null}
      {traits.metered ? (
        <>
          <Toggle label="May spend money" checked={connection.paid} disabled={false} onChange={(paid) => onChange(patchConnection(settings, id, { paid }))} />
          <p className="panel-hint">This connection is billed for each request. While this is off it is never used, even when everything above it has run out.</p>
        </>
      ) : null}
      {traits.experimental ? (
        <>
          <Toggle
            label="Allow this experimental route"
            checked={connection.experimental}
            disabled={false}
            onChange={(experimental) => onChange(patchConnection(settings, id, { experimental }))}
          />
          <p className="panel-hint">
            Requests are translated for another provider’s API. Hidden reasoning is not carried over, and a request the translation cannot carry is refused.
          </p>
        </>
      ) : null}
      {traits.holdsKey ? <ApiKey id={id} /> : null}
      {connection.kind === "anthropic-api-key" ? <p className="panel-hint">Use a key that belongs to one workspace. The building cannot send a workspace id.</p> : null}
      {connection.kind === "openai-api-key" ? <p className="panel-hint">This build cannot use this kind of connection yet, so it is always passed over.</p> : null}
      {connection.kind === "chatgpt-sign-in" ? <p className="panel-hint">Sign in with ChatGPT is not available in this build.</p> : null}
      {traits.passesThrough ? null : (
        <div className="routing-models">
          <h5 className="routing-models-title">Models</h5>
          {TIERS.map((tier) => (
            <TierRow key={tier.id} connection={connection} tier={tier.id} label={tier.label} translated={traits.experimental} settings={settings} onChange={onChange} />
          ))}
          <p className="panel-hint">A tier left empty is not served by this connection.</p>
          <p className="panel-hint">
            {traits.experimental
              ? "Every effort a desk may ask for needs a pair here, or that request is refused."
              : "Efforts are sent unchanged unless a pair here says otherwise."}
          </p>
        </div>
      )}
    </details>
  );
}
