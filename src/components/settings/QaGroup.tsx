import { useEffect, useState } from "react";
import type { QaSettings, ToolOptions } from "../../types";
import { QA_MAX_CONCURRENT, QA_POLL_MINUTES, QA_REVIEWS_PER_DAY } from "../../lib/qaLimits";
import { Toggle } from "../Toggle";

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

const COUNTS: readonly { readonly key: Count; readonly label: string; readonly id: string; readonly unit: string; readonly range: { readonly min: number; readonly max: number }; readonly error: string }[] = [
  { key: "pollMinutes", label: "Check GitHub every", id: "settings-qa-poll", unit: "minutes", range: QA_POLL_MINUTES, error: `Between ${QA_POLL_MINUTES.min} and ${QA_POLL_MINUTES.max} minutes.` },
  { key: "maxConcurrent", label: "Reviews at once", id: "settings-qa-concurrent", unit: "", range: QA_MAX_CONCURRENT, error: `Between ${QA_MAX_CONCURRENT.min} and ${QA_MAX_CONCURRENT.max}.` },
  { key: "reviewsPerDay", label: "Reviews a day for one pull request", id: "settings-qa-per-day", unit: "", range: QA_REVIEWS_PER_DAY, error: `Between ${QA_REVIEWS_PER_DAY.min} and ${QA_REVIEWS_PER_DAY.max}.` },
];

/** The whole number in `text` when it lies in `range`, else null. */
function parseCount(text: string, range: { readonly min: number; readonly max: number }): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const n = Number(text.trim());
  return n >= range.min && n <= range.max ? n : null;
}

/** What the model and effort selects are showing: still looking, ready, or no Claude Code. */
type Models = { readonly kind: "loading" } | { readonly kind: "none" } | { readonly kind: "ready"; readonly options: ToolOptions };

function useClaudeCodeModels(load: () => Promise<readonly ToolOptions[]>): Models {
  const [models, setModels] = useState<Models>({ kind: "loading" });
  useEffect(() => {
    let live = true;
    load()
      .then((all) => {
        const options = all.find((o) => o.tool === "claude-code");
        if (live) setModels(options?.available === true ? { kind: "ready", options } : { kind: "none" });
      })
      .catch(() => {
        if (live) setModels({ kind: "none" });
      });
    return () => {
      live = false;
    };
    // The options load once when the group mounts.
  }, []);
  return models;
}

interface CountFieldProps {
  readonly spec: (typeof COUNTS)[number];
  readonly saved: number;
  readonly onSave: (n: number) => void;
}

/** A whole-number field that saves on blur or Enter, and shows its range error until it is fixed. */
function CountField({ spec, saved, onSave }: CountFieldProps): React.ReactElement {
  const [text, setText] = useState(String(saved));
  const [bad, setBad] = useState(false);
  useEffect(() => {
    setText(String(saved));
    setBad(false);
  }, [saved]);

  const commit = (): void => {
    const n = parseCount(text, spec.range);
    setBad(n === null);
    if (n !== null && n !== saved) onSave(n);
  };

  return (
    <>
      <label className="field">
        <span>{spec.label}</span>
        <span className="settings-cooldown">
          <input
            type="number"
            data-testid={spec.id}
            min={spec.range.min}
            max={spec.range.max}
            step={1}
            inputMode="numeric"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit();
              }
            }}
          />
          {spec.unit}
        </span>
      </label>
      {bad ? (
        <p className="form-error" role="alert">
          {spec.error}
        </p>
      ) : null}
    </>
  );
}

/** How the building's QA desks are run: when, how often, and by which model. */
export function QaGroup({ value, loadOptions, onChange }: Props): React.ReactElement {
  const models = useClaudeCodeModels(loadOptions);
  const listed = models.kind === "ready" ? models.options.models : [];
  const chosen = listed.find((m) => m.id === (value.model ?? (models.kind === "ready" ? models.options.defaultModel : "")));
  const efforts = chosen?.efforts ?? [];
  const disabled = models.kind !== "ready";
  const loading = models.kind === "loading";

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
      <label className="field">
        <span>Model</span>
        <select
          data-testid="settings-qa-model"
          disabled={disabled}
          value={value.model ?? ""}
          onChange={(e) => onChange({ ...value, model: e.target.value === "" ? null : e.target.value, effort: null })}
        >
          {loading ? (
            <option value="">Looking for models…</option>
          ) : (
            <>
              <option value="">Claude Code&apos;s default</option>
              {value.model !== null && !listed.some((m) => m.id === value.model) ? <option value={value.model}>{value.model}</option> : null}
              {listed.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </>
          )}
        </select>
      </label>
      <label className="field">
        <span>Effort</span>
        <select
          data-testid="settings-qa-effort"
          disabled={disabled}
          value={value.effort ?? ""}
          onChange={(e) => onChange({ ...value, effort: e.target.value === "" ? null : e.target.value })}
        >
          {loading ? (
            <option value="">Looking for models…</option>
          ) : (
            <>
              <option value="">The model&apos;s default</option>
              {value.effort !== null && !efforts.some((f) => f.id === value.effort) ? <option value={value.effort}>{value.effort}</option> : null}
              {efforts.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.label}
                </option>
              ))}
            </>
          )}
        </select>
      </label>
      {models.kind === "none" ? <p className="panel-hint">Claude Code was not found on this Mac, so no review can start.</p> : null}
      <p className="panel-hint">
        A floor&apos;s QA desk is switched on from its Info panel. Each review is a Claude Code session on your plan, and it is posted on GitHub from your own account.
      </p>
    </>
  );
}
