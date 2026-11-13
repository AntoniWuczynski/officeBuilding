import { useState } from "react";
import type { HandoverSettings, StartWith, ToolOptions } from "../../types";
import { Toggle, classes } from "../Toggle";
import { useToolModels } from "./ModelPicker";

interface Props {
  readonly value: HandoverSettings;
  readonly loadOptions: () => Promise<readonly ToolOptions[]>;
  readonly onChange: (next: HandoverSettings) => void;
}

type Mode = "not-chosen" | "tool-default" | "chosen";

function modeOf(saved: StartWith | null): Mode {
  return saved === null ? "not-chosen" : saved.kind;
}

const MODES: readonly { readonly mode: Mode; readonly label: string }[] = [
  { mode: "not-chosen", label: "Not chosen" },
  { mode: "tool-default", label: "Codex's own configured default" },
  { mode: "chosen", label: "A model I choose" },
];

/** Handover: the switch, and what Codex starts with when a Claude Code desk is handed to it. */
export function HandoverGroup({ value, loadOptions, onChange }: Props): React.ReactElement {
  const models = useToolModels(loadOptions, "codex");
  const saved = value.claudeToCodex;
  // A half-made pick (a model with no effort yet) lives here, and the saved value stays as it was.
  const [choosing, setChoosing] = useState(saved?.kind === "chosen");
  const [model, setModel] = useState(saved?.kind === "chosen" ? saved.model : "");
  const [effort, setEffort] = useState(saved?.kind === "chosen" ? saved.effort : "");
  const mode: Mode = choosing ? "chosen" : modeOf(saved);

  const listed = models.kind === "ready" ? models.options.models : [];
  const efforts = listed.find((m) => m.id === model)?.efforts ?? [];
  const disabled = models.kind !== "ready";

  const pick = (next: Mode): void => {
    setChoosing(next === "chosen");
    if (next !== "chosen") {
      setModel("");
      setEffort("");
    }
    if (next === "not-chosen") onChange({ ...value, claudeToCodex: null });
    if (next === "tool-default") onChange({ ...value, claudeToCodex: { kind: "tool-default" } });
  };

  return (
    <>
      <Toggle label="Hand desks over by themselves" checked={value.automatic} disabled={false} onChange={(automatic) => onChange({ ...value, automatic })} />
      <fieldset className="choice" data-testid="settings-handover-codex">
        <legend>Claude Code to Codex: Codex starts with</legend>
        {MODES.map(({ mode: m, label }) => (
          <label key={m} className={classes(mode === m && "is-on")}>
            <input type="radio" name="handover-codex" checked={mode === m} onChange={() => pick(m)} />
            {label}
          </label>
        ))}
      </fieldset>
      {mode === "chosen" ? (
        <>
          <label className="field">
            <span>Model</span>
            <select
              data-testid="settings-handover-codex-model"
              disabled={disabled}
              value={model}
              onChange={(e) => {
                setModel(e.target.value);
                setEffort("");
              }}
            >
              <option value="" disabled>
                Choose a model
              </option>
              {model !== "" && !listed.some((m) => m.id === model) ? <option value={model}>{model}</option> : null}
              {listed.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>Effort</span>
            <select
              data-testid="settings-handover-codex-effort"
              disabled={disabled}
              value={effort}
              onChange={(e) => {
                setEffort(e.target.value);
                if (model !== "") onChange({ ...value, claudeToCodex: { kind: "chosen", model, effort: e.target.value } });
              }}
            >
              <option value="" disabled>
                Choose an effort
              </option>
              {effort !== "" && !efforts.some((f) => f.id === effort) ? <option value={effort}>{effort}</option> : null}
              {efforts.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.label}
                </option>
              ))}
            </select>
          </label>
        </>
      ) : null}
      {models.kind === "none" ? <p className="panel-hint">Codex was not found on this Mac, so no desk can be handed to it.</p> : null}
      <p className="panel-hint" data-testid="settings-handover-codex-to-claude">
        Codex to Claude Code: not built yet. A Codex desk is never handed over in this version.
      </p>
      <p className="panel-hint">
        When a desk&apos;s Claude subscription is used up, the building ends its Claude Code session and starts Codex in the same folder, on your own Codex sign-in and with your own Codex settings for sandbox and approvals. A desk that has a workflow or other background work running, or where the building cannot tell, is never handed over by itself: it gets a Hand over button, and using it ends that work. With the switch off, every used-up desk gets the button. Nothing is handed over until you have chosen what Codex starts with.
      </p>
      <p className="panel-hint">
        A handover writes the desk&apos;s visible conversation, with tool output cut short and a summary of the folder&apos;s git state, to a file only you can read in the building&apos;s data folder, and tells Codex to read it. So what was said at the desk goes to the other vendor, including any secret a tool printed. Hidden reasoning is left out. No login is read or moved.
      </p>
    </>
  );
}
