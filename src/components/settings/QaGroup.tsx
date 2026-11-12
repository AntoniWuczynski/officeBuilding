import type { QaSettings, ToolOptions } from "../../types";
import { QA_MAX_CONCURRENT, QA_POLL_MINUTES, QA_REVIEWS_PER_DAY } from "../../lib/qaLimits";
import { Toggle } from "../Toggle";
import { CountField, type CountSpec } from "./CountField";
import { ModelPicker } from "./ModelPicker";

interface Props {
  readonly value: QaSettings;
  readonly loadOptions: () => Promise<readonly ToolOptions[]>;
  readonly onChange: (next: QaSettings) => void;
}

type Switch = "defaultOn" | "everyPush" | "skipDrafts" | "postWhenClean";
type Count = "pollMinutes" | "maxConcurrent" | "reviewsPerDay";

const SWITCHES: readonly { readonly key: Switch; readonly label: string; readonly id: string }[] = [
  { key: "defaultOn", label: "On for floors you have not switched", id: "settings-qa-default" },
  { key: "everyPush", label: "Review every push", id: "settings-qa-every-push" },
  { key: "skipDrafts", label: "Skip drafts", id: "settings-qa-skip-drafts" },
  { key: "postWhenClean", label: "Post when nothing is found", id: "settings-qa-post-clean" },
];

const COUNTS: readonly (CountSpec & { readonly key: Count })[] = [
  { key: "pollMinutes", label: "Check GitHub every", id: "settings-qa-poll", unit: "minutes", range: QA_POLL_MINUTES, error: `Between ${QA_POLL_MINUTES.min} and ${QA_POLL_MINUTES.max} minutes.` },
  { key: "maxConcurrent", label: "Reviews at once", id: "settings-qa-concurrent", unit: "", range: QA_MAX_CONCURRENT, error: `Between ${QA_MAX_CONCURRENT.min} and ${QA_MAX_CONCURRENT.max}.` },
  { key: "reviewsPerDay", label: "Reviews a day for one pull request", id: "settings-qa-per-day", unit: "", range: QA_REVIEWS_PER_DAY, error: `Between ${QA_REVIEWS_PER_DAY.min} and ${QA_REVIEWS_PER_DAY.max}.` },
];

/** How the building's QA desks are run: when, how often, and by which model. */
export function QaGroup({ value, loadOptions, onChange }: Props): React.ReactElement {
  return (
    <>
      {SWITCHES.map((row) => (
        <div key={row.key} data-testid={row.id}>
          <Toggle label={row.label} checked={value[row.key]} disabled={false} onChange={(on) => onChange({ ...value, [row.key]: on })} />
          {row.key === "everyPush" && !value.everyPush ? <p className="panel-hint">Only the first commit of each pull request is reviewed.</p> : null}
        </div>
      ))}
      {COUNTS.map((spec) => (
        <CountField key={spec.key} spec={spec} saved={value[spec.key]} onSave={(n) => onChange({ ...value, [spec.key]: n })} />
      ))}
      <label className="field">
        <span>Reviewer</span>
        <select data-testid="settings-qa-tool" value={value.tool} onChange={() => onChange({ ...value, tool: "claude-code" })}>
          <option value="claude-code">Claude Code</option>
        </select>
      </label>
      <ModelPicker
        idPrefix="settings-qa"
        model={value.model}
        effort={value.effort}
        loadOptions={loadOptions}
        onChange={(pick) => onChange({ ...value, ...pick })}
        missing={<p className="panel-hint">Claude Code was not found on this Mac, so no review can start.</p>}
      />
      <p className="panel-hint">
        A floor&apos;s QA desk is switched on from its Info panel. Each review is a Claude Code session on your plan, and it is posted on GitHub from your own account.
      </p>
    </>
  );
}
