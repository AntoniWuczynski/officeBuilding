import { useEffect, useState } from "react";
import type { ToolOptions } from "../../types";

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

interface Props {
  /** Gives the test ids `<prefix>-model` and `<prefix>-effort`. */
  readonly idPrefix: string;
  readonly model: string | null;
  readonly effort: string | null;
  readonly loadOptions: () => Promise<readonly ToolOptions[]>;
  readonly onChange: (next: { readonly model: string | null; readonly effort: string | null }) => void;
  /** Shown under the selects when Claude Code was not found. */
  readonly missing: React.ReactNode;
}

/** The Model and Effort selects for a Claude Code agent. Picking a model resets the effort. */
export function ModelPicker({ idPrefix, model, effort, loadOptions, onChange, missing }: Props): React.ReactElement {
  const models = useClaudeCodeModels(loadOptions);
  const listed = models.kind === "ready" ? models.options.models : [];
  const chosen = listed.find((m) => m.id === (model ?? (models.kind === "ready" ? models.options.defaultModel : "")));
  const efforts = chosen?.efforts ?? [];
  const disabled = models.kind !== "ready";
  const loading = models.kind === "loading";

  return (
    <>
      <label className="field">
        <span>Model</span>
        <select
          data-testid={`${idPrefix}-model`}
          disabled={disabled}
          value={model ?? ""}
          onChange={(e) => onChange({ model: e.target.value === "" ? null : e.target.value, effort: null })}
        >
          {loading ? (
            <option value="">Looking for models…</option>
          ) : (
            <>
              <option value="">Claude Code&apos;s default</option>
              {model !== null && !listed.some((m) => m.id === model) ? <option value={model}>{model}</option> : null}
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
          data-testid={`${idPrefix}-effort`}
          disabled={disabled}
          value={effort ?? ""}
          onChange={(e) => onChange({ model, effort: e.target.value === "" ? null : e.target.value })}
        >
          {loading ? (
            <option value="">Looking for models…</option>
          ) : (
            <>
              <option value="">The model&apos;s default</option>
              {effort !== null && !efforts.some((f) => f.id === effort) ? <option value={effort}>{effort}</option> : null}
              {efforts.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.label}
                </option>
              ))}
            </>
          )}
        </select>
      </label>
      {models.kind === "none" ? missing : null}
    </>
  );
}
