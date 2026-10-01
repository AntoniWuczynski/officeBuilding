import { useEffect, useState } from "react";
import type { ModelOption, ToolKind, ToolOptions } from "../../types";
import type { HireRequest } from "../../api/DiscoveryApi";
import { TOOL_LABEL } from "../../state/selectors";
import { message } from "../../lib/errors";

type Load =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly options: readonly ToolOptions[] }
  | { readonly status: "error"; readonly message: string };

interface Props {
  readonly projectName: string;
  readonly loadOptions: () => Promise<readonly ToolOptions[]>;
  readonly onHire: (request: HireRequest) => Promise<void>;
  readonly onCancel: () => void;
}

interface Choice {
  readonly tool: ToolKind;
  readonly model: string;
  readonly effort: string;
}

function defaultsFor(tool: ToolOptions): Choice {
  const model = tool.models.find((m) => m.id === tool.defaultModel) ?? tool.models[0];
  return { tool: tool.tool, model: model?.id ?? "", effort: model?.defaultEffort ?? "" };
}

/** Keep the effort if the new model supports it, otherwise take the model's default. */
function effortFor(model: ModelOption, current: string): string {
  return model.efforts.some((e) => e.id === current) ? current : model.defaultEffort;
}

/** Start a new agent session on this floor: tool, model, effort and task. */
export function HireDialog({ projectName, loadOptions, onHire, onCancel }: Props): React.ReactElement {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [choice, setChoice] = useState<Choice | null>(null);
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    loadOptions()
      .then((options) => {
        if (!live) return;
        setLoad({ status: "ready", options });
        const first = options[0];
        setChoice((c) => c ?? (first === undefined ? null : defaultsFor(first)));
      })
      .catch((err: unknown) => {
        if (live) setLoad({ status: "error", message: message(err) });
      });
    return () => {
      live = false;
    };
  }, [loadOptions, attempt]);

  const options = load.status === "ready" ? load.options : [];
  const tool = options.find((o) => o.tool === choice?.tool);
  const model = tool?.models.find((m) => m.id === choice?.model);
  const effort = model?.efforts.find((e) => e.id === choice?.effort);

  return (
    <form
      className="dialog"
      role="dialog"
      aria-labelledby="hire-title"
      data-testid="hire-dialog"
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      onSubmit={(e) => {
        e.preventDefault();
        if (choice === null) return;
        setBusy(true);
        setError(null);
        onHire({ ...choice, title }).catch((err: unknown) => {
          setBusy(false);
          setError(message(err));
        });
      }}
    >
      <h2 id="hire-title">Hire an agent for {projectName}</h2>

      {load.status === "loading" ? <p className="panel-hint">Loading the tools and models you can hire…</p> : null}
      {load.status === "error" ? (
        <div className="form-error" role="alert">
          Could not load the tools: {load.message}.{" "}
          <button
            type="button"
            className="link"
            onClick={() => {
              setLoad({ status: "loading" });
              setAttempt((n) => n + 1);
            }}
          >
            Try again
          </button>
        </div>
      ) : null}

      {tool !== undefined && model !== undefined ? (
        <>
          <fieldset className="choice" disabled={busy}>
            <legend>Tool</legend>
            {options.map((o) => (
              <label key={o.tool} className={o.tool === tool.tool ? "is-on" : undefined}>
                <input type="radio" name="tool" checked={o.tool === tool.tool} onChange={() => setChoice(defaultsFor(o))} />
                {TOOL_LABEL[o.tool]}
              </label>
            ))}
          </fieldset>

          <label className="field">
            <span>Model</span>
            <select
              value={model.id}
              disabled={busy}
              data-testid="hire-model"
              onChange={(e) => {
                const next = tool.models.find((m) => m.id === e.target.value);
                if (next !== undefined) setChoice({ tool: tool.tool, model: next.id, effort: effortFor(next, choice?.effort ?? "") });
              }}
            >
              {tool.models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </select>
          </label>

          <fieldset className="choice" disabled={busy}>
            <legend>Effort</legend>
            {model.efforts.map((e) => (
              <label key={e.id} className={e.id === effort?.id ? "is-on" : undefined}>
                <input
                  type="radio"
                  name="effort"
                  checked={e.id === effort?.id}
                  onChange={() => setChoice({ tool: tool.tool, model: model.id, effort: e.id })}
                />
                {e.label}
              </label>
            ))}
          </fieldset>
          {effort !== undefined && effort.description !== "" ? <p className="panel-hint">{effort.description}.</p> : null}

          <label className="field">
            <span>Task</span>
            <input value={title} disabled={busy} placeholder="What should they work on?" onChange={(e) => setTitle(e.target.value)} autoFocus />
          </label>
        </>
      ) : null}

      {error === null ? null : (
        <p className="form-error" role="alert">
          Could not hire: {error}. Check the choices, then try again.
        </p>
      )}
      <div className="dialog-actions">
        <button type="button" className="btn" onClick={onCancel} disabled={busy}>
          Cancel
        </button>
        <button type="submit" className="btn btn-primary" data-testid="hire-submit" disabled={busy || model === undefined}>
          {busy ? "Hiring" : "Hire"}
        </button>
      </div>
    </form>
  );
}
