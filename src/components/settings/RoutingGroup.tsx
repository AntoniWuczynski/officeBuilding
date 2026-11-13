import { useState } from "react";
import type { RoutingSettings } from "../../types";
import { CONTEXTS_MAX, TIERS, addContext, cleanName, newId, parseModels, setTierModels } from "../../lib/routing";
import { CommitField } from "./CommitField";
import { ContextCard } from "../routing/ContextCard";
import { Toggle } from "../Toggle";

interface Props {
  readonly value: RoutingSettings;
  readonly onChange: (next: RoutingSettings) => void;
}

/** Routing: the switch, the account contexts with their connections, and the model names by tier. */
export function RoutingGroup({ value, onChange }: Props): React.ReactElement {
  const [name, setName] = useState("");
  const clean = cleanName(name);

  const add = (): void => {
    if (clean === null) return;
    onChange(addContext(value, newId("ctx", value), clean));
    setName("");
  };

  return (
    <>
      <Toggle label="Route Claude Code desks through the building" checked={value.enabled} disabled={false} onChange={(enabled) => onChange({ ...value, enabled })} />
      <p className="panel-hint">Off: every desk starts exactly as it does today. On: a Claude Code desk the building starts on a floor that has an account context sends its requests through the building, which passes them to the first connection of that context’s order and moves on only when a connection reports that its allowance is used up. Desks that are already running are not changed either way. A routed desk needs the building: if the building quits, the desk loses its model access.</p>

      <h4 className="panel-subhead">Account contexts</h4>
      <p className="panel-hint">
        A context keeps one set of accounts apart from another, for example work and personal. A floor takes its context on its Info panel, and a desk only ever uses the connections of its floor’s context.
      </p>
      {value.contexts.map((context) => (
        <ContextCard key={context.id} context={context} settings={value} onChange={onChange} />
      ))}
      <div className="routing-add">
        <label className="field">
          <span>New context</span>
          <input
            type="text"
            data-testid="routing-new-context"
            placeholder="Name, for example Work"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                add();
              }
            }}
          />
        </label>
        <button type="button" className="btn" disabled={clean === null || value.contexts.length >= CONTEXTS_MAX} onClick={add}>
          Add context
        </button>
      </div>

      <h4 className="panel-subhead">Model tiers</h4>
      <p className="panel-hint">The model names a desk asks for, by tier, separated by commas. A name is matched exactly. Nothing is filled in for you.</p>
      {TIERS.map((tier) => (
        <CommitField
          key={tier.id}
          label={tier.label}
          testId={`routing-tier-${tier.id}`}
          value={value.tiers[tier.id].join(", ")}
          onCommit={(text) => {
            onChange(setTierModels(value, tier.id, parseModels(text)));
            return null;
          }}
        />
      ))}
    </>
  );
}
