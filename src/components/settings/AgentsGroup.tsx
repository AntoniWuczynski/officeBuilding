import type { AgentSettings, DeskView } from "../../types";
import { classes } from "../Toggle";

interface Props {
  readonly value: AgentSettings;
  readonly onChange: (next: AgentSettings) => void;
}

const VIEWS: readonly { readonly view: DeskView; readonly label: string }[] = [
  { view: "terminal", label: "Terminal" },
  { view: "chat", label: "Chat" },
];

/** Which view new desks open in. */
export function AgentsGroup({ value, onChange }: Props): React.ReactElement {
  return (
    <>
      <fieldset className="choice" data-testid="settings-view">
        <legend>View</legend>
        {VIEWS.map(({ view, label }) => (
          <label key={view} className={classes(value.view === view && "is-on")}>
            <input type="radio" name="agents-view" checked={value.view === view} onChange={() => onChange({ view })} />
            {label}
          </label>
        ))}
      </fieldset>
      <p className="panel-hint">New desks open in this view. The hire dialog can change it for one hire. A tool with no chat view stays in the terminal.</p>
    </>
  );
}
