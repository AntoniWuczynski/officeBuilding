import { useState } from "react";
import type { HandoverSettings, HandoverTool, StartWith, ToolOptions } from "../../types";
import { TOOL_LABEL } from "../../state/selectors";
import { otherTool } from "../../lib/handover";
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

const MODES: readonly { readonly mode: Mode; readonly label: (to: string) => string }[] = [
  { mode: "not-chosen", label: () => "Not chosen" },
  { mode: "tool-default", label: (to) => `${to}'s own configured default` },
  { mode: "chosen", label: () => "A model I choose" },
];

interface ChoiceProps {
  /** The tool that starts: `claudeToCodex` is the choice for "codex". */
  readonly to: HandoverTool;
  readonly saved: StartWith | null;
  readonly loadOptions: () => Promise<readonly ToolOptions[]>;
  readonly onChange: (next: StartWith | null) => void;
}

/** One direction: what the receiving tool starts with. */
function ReceiverChoice({ to, saved, loadOptions, onChange }: ChoiceProps): React.ReactElement {
  const models = useToolModels(loadOptions, to);
  const label = TOOL_LABEL[to];
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
    if (next === "not-chosen") onChange(null);
    if (next === "tool-default") onChange({ kind: "tool-default" });
  };

  return (
    <>
      <fieldset className="choice" data-testid={`settings-handover-${to}`}>
        <legend>{`${TOOL_LABEL[otherTool(to)]} to ${label}: ${label} starts with`}</legend>
        {MODES.map(({ mode: m, label: describe }) => (
          <label key={m} className={classes(mode === m && "is-on")}>
            <input type="radio" name={`handover-${to}`} checked={mode === m} onChange={() => pick(m)} />
            {describe(label)}
          </label>
        ))}
      </fieldset>
      {mode === "chosen" ? (
        <>
          <label className="field">
            <span>Model</span>
            <select
              data-testid={`settings-handover-${to}-model`}
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
              data-testid={`settings-handover-${to}-effort`}
              disabled={disabled}
              value={effort}
              onChange={(e) => {
                setEffort(e.target.value);
                if (model !== "") onChange({ kind: "chosen", model, effort: e.target.value });
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
      {models.kind === "none" ? <p className="panel-hint">{label} was not found on this Mac, so no desk can be handed to it.</p> : null}
    </>
  );
}

/** Handover: the switch, and what each tool starts with when a desk is handed to it. */
export function HandoverGroup({ value, loadOptions, onChange }: Props): React.ReactElement {
  return (
    <>
      <Toggle label="Hand desks over by themselves" checked={value.automatic} disabled={false} onChange={(automatic) => onChange({ ...value, automatic })} />
      <ReceiverChoice to="codex" saved={value.claudeToCodex} loadOptions={loadOptions} onChange={(claudeToCodex) => onChange({ ...value, claudeToCodex })} />
      <ReceiverChoice to="claude-code" saved={value.codexToClaude} loadOptions={loadOptions} onChange={(codexToClaude) => onChange({ ...value, codexToClaude })} />
      <p className="panel-hint">
        When a desk&apos;s own tool records that its allowance is used up, the building ends that session and starts the other tool in the same folder, on your own sign-in and with your own settings for permissions, sandbox and approvals. For Claude Code that record is the Claude subscription&apos;s limit. For Codex it is Codex&apos;s usage limit on its own OpenAI provider. A desk with background work running, or where the building cannot tell, is never handed over by itself: it gets a Hand over button, and using it ends that work. In Claude Code that work is a workflow, a background subagent or shell, or a scheduled wake-up. In Codex it is a goal that is neither paused nor complete, or a subagent with a turn still open. With the switch off, every used-up desk gets the button. Nothing is handed over in a direction until you have chosen what the receiving tool starts with.
      </p>
      <p className="panel-hint">
        A handover writes the desk&apos;s visible conversation, with tool output cut short and a summary of the folder&apos;s git state, to a file only you can read in the building&apos;s data folder, and tells the receiving tool to read it. Claude Code is given leave to read that one file. So what was said at the desk goes to the other vendor, including any secret a tool printed. Hidden reasoning is left out. No login is read or moved.
      </p>
      <p className="panel-hint">
        The building reads Codex&apos;s limit from the session&apos;s own history file. It knows when the limit resets only if Codex recorded a full window, and holds the record for five hours otherwise. A goal you cleared still reads as set, so such a desk waits for you.
      </p>
    </>
  );
}
