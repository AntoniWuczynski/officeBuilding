import type { ManagerSettings, ToolOptions } from "../../types";
import { MANAGER_MAX_DESKS, MANAGER_SPEND_CEILING_USD } from "../../lib/managerLimits";
import { CountField, type CountSpec } from "./CountField";
import { ModelPicker } from "./ModelPicker";

interface Props {
  readonly value: ManagerSettings;
  readonly loadOptions: () => Promise<readonly ToolOptions[]>;
  readonly onChange: (next: ManagerSettings) => void;
}

const MAX_DESKS: CountSpec = {
  label: "Desks a manager may hire at once",
  id: "settings-manager-max-desks",
  unit: "",
  range: MANAGER_MAX_DESKS,
  error: `Between ${MANAGER_MAX_DESKS.min} and ${MANAGER_MAX_DESKS.max}.`,
};
const CEILING: CountSpec = {
  label: "Spend ceiling for desks it hired",
  id: "settings-manager-ceiling",
  unit: "dollars",
  range: MANAGER_SPEND_CEILING_USD,
  error: `Between ${MANAGER_SPEND_CEILING_USD.min} and ${MANAGER_SPEND_CEILING_USD.max} dollars.`,
};

/** How the building's floor managers are run: by which tool and model, and how far they may go. */
export function ManagerGroup({ value, loadOptions, onChange }: Props): React.ReactElement {
  return (
    <>
      <label className="field">
        <span>Manager</span>
        <select data-testid="settings-manager-tool" value={value.tool} onChange={() => onChange({ ...value, tool: "claude-code" })}>
          <option value="claude-code">Claude Code</option>
        </select>
      </label>
      <ModelPicker
        idPrefix="settings-manager"
        model={value.model}
        effort={value.effort}
        loadOptions={loadOptions}
        onChange={(pick) => onChange({ ...value, ...pick })}
        missing={<p className="panel-hint">Claude Code was not found on this Mac, so no manager can be hired.</p>}
      />
      <CountField spec={MAX_DESKS} saved={value.maxDesks} onSave={(maxDesks) => onChange({ ...value, maxDesks })} />
      <CountField spec={CEILING} saved={value.spendCeilingUsd} onSave={(spendCeilingUsd) => onChange({ ...value, spendCeilingUsd })} />
      <p className="panel-hint">
        A manager is hired from the manager&apos;s office on a floor. It reads the floor and hands work to its desks, on your Claude plan. It cannot hire desks on a floor until you switch that on in the floor&apos;s Manager panel. Model and effort apply to the next manager you hire.
      </p>
    </>
  );
}
