import { useEffect, useState } from "react";
import type { DeskView, HandoverTool, ModelOption, TakeOverStart, ToolKind, ToolOptions } from "../../types";
import type { HireRequest } from "../../api/DiscoveryApi";
import { TOOL_LABEL } from "../../state/selectors";
import { message } from "../../lib/errors";
import { takeOverConfirm } from "../../lib/handover";
import { Toggle, classes } from "../Toggle";

type Load =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly options: readonly ToolOptions[] }
  | { readonly status: "error"; readonly message: string };

interface CommonProps {
  readonly loadOptions: () => Promise<readonly ToolOptions[]>;
  readonly onCancel: () => void;
}

type HireProps = CommonProps & { readonly projectName: string; readonly onHire: (request: HireRequest) => Promise<void>; readonly takeOver?: undefined };
type TakeOverProps = CommonProps & { readonly takeOver: { readonly title: string }; readonly onTakeOver: (start: TakeOverStart) => Promise<void> };
/** Hiring on a floor, or (with `takeOver`) starting a new session beside the one named. */
type Props = HireProps | TakeOverProps;

interface Choice {
  readonly tool: ToolKind;
  readonly model: string;
  readonly effort: string;
  readonly ultracode: boolean;
  readonly view: DeskView;
}

function defaultsFor(tool: ToolOptions): Choice {
  const model =
    tool.models.find((m) => m.id === tool.defaultModel) ?? tool.models[0];
  return {
    tool: tool.tool,
    model: model?.id ?? "",
    effort: model?.defaultEffort ?? "",
    ultracode: tool.ultracode ?? false,
    view: tool.view,
  };
}

/** The line under the effort: why chat is off, or what a chat desk is like. */
function viewHint(tool: ToolOptions, view: DeskView): string | null {
  if (tool.chat.kind === "unavailable") return `Chat is not available: ${tool.chat.reason}.`;
  if (view !== "chat") return null;
  return tool.chat.terminal
    ? "The desk keeps its terminal. Its panel opens as a chat, and you can flip between the two at any time."
    : "It runs without a terminal, and the view cannot be changed after the hire.";
}

/** Keep the effort if the new model supports it, otherwise take the model's default. */
function effortFor(model: ModelOption, current: string): string {
  return model.efforts.some((e) => e.id === current)
    ? current
    : model.defaultEffort;
}

/** The tools and models on offer, loaded once, with a way to load them again. */
function useToolOptions(loadOptions: () => Promise<readonly ToolOptions[]>): { readonly load: Load; readonly retry: () => void } {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let live = true;
    loadOptions()
      .then((options) => {
        if (live) setLoad({ status: "ready", options });
      })
      .catch((err: unknown) => {
        if (live) setLoad({ status: "error", message: message(err) });
      });
    return () => {
      live = false;
    };
  }, [loadOptions, attempt]);
  return {
    load,
    retry: () => {
      setLoad({ status: "loading" });
      setAttempt((n) => n + 1);
    },
  };
}

/** What the sheet says while the options load or fail to load. */
function LoadNotice({ load, retry }: { readonly load: Load; readonly retry: () => void }): React.ReactElement | null {
  if (load.status === "loading") return <p className="panel-hint">Loading the tools and models you can hire…</p>;
  if (load.status === "ready") return null;
  return (
    <div className="form-error" role="alert">
      Could not load the tools: {load.message}.{" "}
      <button type="button" className="link" onClick={retry}>
        Try again
      </button>
    </div>
  );
}

/** The dialog form both sheets share: Escape cancels, a submit runs `onSubmit`. */
function DialogShell({ onCancel, onSubmit, children }: { readonly onCancel: () => void; readonly onSubmit: () => void; readonly children: React.ReactNode }): React.ReactElement {
  return (
    <form
      className="dialog"
      role="dialog"
      aria-labelledby="hire-title"
      data-testid="hire-dialog"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          onCancel();
        }
      }}
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit();
      }}
    >
      {children}
    </form>
  );
}

/** The Tool fieldset: one radio per tool, an uninstalled one disabled. */
function ToolRadios({ options, selected, busy, onPick }: { readonly options: readonly ToolOptions[]; readonly selected: ToolKind | null; readonly busy: boolean; readonly onPick: (tool: ToolOptions) => void }): React.ReactElement {
  return (
    <fieldset className="choice" disabled={busy}>
      <legend>Tool</legend>
      {options.map((o) => (
        <label key={o.tool} className={classes(o.tool === selected && "is-on", !o.available && "is-off")}>
          <input type="radio" name="tool" checked={o.tool === selected} disabled={!o.available} onChange={() => onPick(o)} />
          {TOOL_LABEL[o.tool]}
          {o.available ? null : <small className="choice-note">Not installed</small>}
        </label>
      ))}
    </fieldset>
  );
}

/** The Effort fieldset of a model. */
function EffortRadios({ model, selected, busy, onPick }: { readonly model: ModelOption; readonly selected: string | undefined; readonly busy: boolean; readonly onPick: (effort: string) => void }): React.ReactElement {
  return (
    <fieldset className="choice" disabled={busy}>
      <legend>Effort</legend>
      {model.efforts.map((e) => (
        <label key={e.id} className={e.id === selected ? "is-on" : undefined}>
          <input type="radio" name="effort" checked={e.id === selected} onChange={() => onPick(e.id)} />
          {e.label}
        </label>
      ))}
    </fieldset>
  );
}

/** Start a new agent session on this floor: tool, model, effort and task. Or, in take-over mode, a new session beside an old one. */
export function HireDialog(props: Props): React.ReactElement {
  return props.takeOver === undefined ? <HireForm {...props} /> : <TakeOverForm {...props} />;
}

function HireForm({ projectName, loadOptions, onHire, onCancel }: HireProps): React.ReactElement {
  const { load, retry } = useToolOptions(loadOptions);
  const [choice, setChoice] = useState<Choice | null>(null);
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (load.status !== "ready") return;
    const first = load.options.find((o) => o.available);
    setChoice((c) => c ?? (first === undefined ? null : defaultsFor(first)));
  }, [load]);

  const options = load.status === "ready" ? load.options : [];
  const tool = options.find((o) => o.tool === choice?.tool);
  const model = tool?.models.find((m) => m.id === choice?.model);
  const effort = model?.efforts.find((e) => e.id === choice?.effort);
  const hint = tool === undefined || choice === null ? null : viewHint(tool, choice.view);

  const submit = (): void => {
    if (choice === null) return;
    setBusy(true);
    setError(null);
    onHire({ ...choice, title }).catch((err: unknown) => {
      setBusy(false);
      setError(message(err));
    });
  };

  return (
    <DialogShell onCancel={onCancel} onSubmit={submit}>
      <h2 id="hire-title">Hire an agent for {projectName}</h2>

      <LoadNotice load={load} retry={retry} />

      {choice !== null && tool !== undefined ? (
        <ToolRadios options={options} selected={tool.tool} busy={busy} onPick={(o) => setChoice(defaultsFor(o))} />
      ) : null}

      {choice !== null && tool !== undefined && model !== undefined ? (
        <>
          <label className="field">
            <span>Model</span>
            <select
              value={model.id}
              disabled={busy}
              data-testid="hire-model"
              onChange={(e) => {
                const next = tool.models.find((m) => m.id === e.target.value);
                if (next !== undefined)
                  setChoice({
                    ...choice,
                    model: next.id,
                    effort: effortFor(next, choice.effort),
                  });
              }}
            >
              {tool.models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </select>
          </label>

          <div className="choice-row">
            <EffortRadios model={model} selected={effort?.id} busy={busy} onPick={(id) => setChoice({ ...choice, effort: id })} />
            <div className="choice choice-end">
              {tool.ultracode === null ? null : (
                <Toggle label="Ultracode" checked={choice.ultracode} disabled={busy} onChange={(on) => setChoice({ ...choice, ultracode: on })} />
              )}
              <Toggle
                label="Chat view"
                {...(hint === null ? {} : { title: hint })}
                checked={choice.view === "chat"}
                disabled={busy || tool.chat.kind === "unavailable"}
                onChange={(on) => setChoice({ ...choice, view: on ? "chat" : "terminal" })}
              />
            </div>
          </div>
          {effort !== undefined && effort.description !== "" ? (
            <p className="panel-hint">{effort.description}.</p>
          ) : null}
          {hint === null ? null : (
            <p className="panel-hint" data-testid="hire-view-hint">
              {hint}
            </p>
          )}

          <label className="field">
            <span>Task</span>
            <input
              value={title}
              disabled={busy}
              placeholder="What should they work on?"
              onChange={(e) => setTitle(e.target.value)}
              autoFocus
            />
          </label>
        </>
      ) : null}

      {error === null ? null : (
        <p className="form-error" role="alert">
          Could not hire: {error}. Check the choices, then try again.
        </p>
      )}
      <div className="dialog-actions">
        <button
          type="button"
          className="btn"
          onClick={onCancel}
          disabled={busy}
        >
          Cancel
        </button>
        <button
          type="submit"
          className="btn btn-primary"
          data-testid="hire-submit"
          disabled={busy || model === undefined}
        >
          {busy ? "Hiring" : "Hire"}
        </button>
      </div>
    </DialogShell>
  );
}

const isHandoverTool = (o: ToolOptions): o is ToolOptions & { readonly tool: HandoverTool } => o.tool === "claude-code" || o.tool === "codex";

interface Pick {
  readonly tool: HandoverTool | null;
  readonly model: string;
  readonly effort: string;
}

/** The Hire sheet in take-over mode: Claude Code or Codex, and a model and an effort that nobody has chosen for you. */
function TakeOverForm({ takeOver, loadOptions, onTakeOver, onCancel }: TakeOverProps): React.ReactElement {
  const { load, retry } = useToolOptions(loadOptions);
  const [pick, setPick] = useState<Pick>({ tool: null, model: "", effort: "" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const options = load.status === "ready" ? load.options.filter(isHandoverTool) : [];
  const tool = options.find((o) => o.tool === pick.tool);
  const model = tool?.models.find((m) => m.id === pick.model);
  const effort = model?.efforts.find((e) => e.id === pick.effort);

  const chooseModel = (next: ModelOption): void => {
    const only = next.efforts.length === 1 ? next.efforts[0] : undefined;
    const kept = next.efforts.some((e) => e.id === pick.effort) ? pick.effort : (only?.id ?? "");
    setPick({ ...pick, model: next.id, effort: kept });
  };

  const submit = (): void => {
    if (tool === undefined || model === undefined || effort === undefined) return;
    setBusy(true);
    setError(null);
    onTakeOver({ tool: tool.tool, model: model.id, effort: effort.id }).catch((err: unknown) => {
      setBusy(false);
      setError(message(err));
    });
  };

  return (
    <DialogShell onCancel={onCancel} onSubmit={submit}>
      <h2 id="hire-title">Take over “{takeOver.title}” with a new session</h2>

      <LoadNotice load={load} retry={retry} />

      {load.status === "ready" ? (
        <ToolRadios options={options} selected={pick.tool} busy={busy} onPick={(o) => setPick({ tool: isHandoverTool(o) ? o.tool : null, model: "", effort: "" })} />
      ) : null}

      {tool === undefined ? null : (
        <label className="field">
          <span>Model</span>
          <select
            value={model?.id ?? ""}
            disabled={busy}
            data-testid="hire-model"
            onChange={(e) => {
              const next = tool.models.find((m) => m.id === e.target.value);
              if (next !== undefined) chooseModel(next);
            }}
          >
            <option value="" disabled>
              Choose a model
            </option>
            {tool.models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
        </label>
      )}

      {model === undefined ? null : (
        <>
          <EffortRadios model={model} selected={effort?.id} busy={busy} onPick={(id) => setPick({ ...pick, effort: id })} />
          {effort !== undefined && effort.description !== "" ? <p className="panel-hint">{effort.description}.</p> : null}
        </>
      )}

      <p className="panel-hint" data-testid="take-over-confirm">
        {takeOverConfirm(pick.tool)}
      </p>

      {error === null ? null : (
        <p className="form-error" role="alert">
          Could not take over: {error}.
        </p>
      )}
      <div className="dialog-actions">
        <button type="button" className="btn" onClick={onCancel} disabled={busy}>
          Cancel
        </button>
        <button type="submit" className="btn btn-primary" data-testid="hire-submit" disabled={busy || effort === undefined}>
          {busy ? "Starting" : "Start and take over"}
        </button>
      </div>
    </DialogShell>
  );
}
