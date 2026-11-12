import { useContext, useEffect, useState } from "react";
import type { ConnectorSettings, LinearKeyStatus } from "../../types";
import { message } from "../../lib/errors";
import { AGENT_LABEL_MAX } from "../../lib/agentLabel";
import { ConnectorsContext } from "../connectors/ConnectorsContext";
import type { ConnectorActions } from "../connectors/ConnectorsContext";

const LABEL_ERROR = `Enter a label of 1 to ${AGENT_LABEL_MAX} characters.`;

type KeyState =
  | { readonly phase: "reading" }
  | { readonly phase: "failed"; readonly message: string }
  | { readonly phase: "ready"; readonly status: LinearKeyStatus; readonly saving: boolean; readonly saveError: string | null };

function statusSentence(status: LinearKeyStatus): string {
  switch (status.kind) {
    case "missing":
      return "No Linear API key is saved.";
    case "saved":
      return `A Linear API key is saved in your Keychain.${status.account === null ? "" : ` It belongs to ${status.account}.`}`;
    case "rejected":
      return "Linear did not accept the saved key. Paste a new one.";
    case "unavailable":
      return `The Keychain did not hand over the key: ${status.reason}.`;
  }
}

function KeyField({ actions }: { readonly actions: ConnectorActions }): React.ReactElement {
  const [state, setState] = useState<KeyState>({ phase: "reading" });
  const [text, setText] = useState("");

  useEffect(() => {
    let live = true;
    actions.keyStatus().then(
      (status) => live && setState({ phase: "ready", status, saving: false, saveError: null }),
      (err: unknown) => live && setState({ phase: "failed", message: message(err) }),
    );
    return () => {
      live = false;
    };
  }, [actions]);

  const run = (key: string | null, status: LinearKeyStatus): void => {
    setState({ phase: "ready", status, saving: true, saveError: null });
    actions.setKey(key).then(
      (next) => setState({ phase: "ready", status: next, saving: false, saveError: null }),
      (err: unknown) => setState({ phase: "ready", status, saving: false, saveError: message(err) }),
    );
  };

  if (state.phase === "reading") {
    return (
      <p className="panel-hint" role="status">
        Reading the key’s status…
      </p>
    );
  }
  if (state.phase === "failed") {
    return (
      <p className="form-error" role="alert">
        Could not read the key’s status: {state.message}.
      </p>
    );
  }
  const { status, saving, saveError } = state;
  const save = (): void => {
    const key = text;
    setText("");
    run(key, status);
  };
  return (
    <>
      <p className="panel-hint" role="status" data-testid="linear-key-status">
        {saving ? "Checking the key with Linear…" : statusSentence(status)}
      </p>
      {saveError === null ? null : (
        <p className="form-error" role="alert">
          Could not save the key: {saveError}.
        </p>
      )}
      <label className="field">
        <span>API key</span>
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          data-testid="linear-key"
          placeholder="Paste a Linear API key"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && text.trim() !== "" && !saving) {
              e.preventDefault();
              save();
            }
          }}
        />
      </label>
      <div className="settings-test">
        <button type="button" className="btn" data-testid="linear-key-save" disabled={text.trim() === "" || saving} onClick={save}>
          Save key
        </button>
        {status.kind === "saved" || status.kind === "rejected" ? (
          <button type="button" className="btn" data-testid="linear-key-remove" disabled={saving} onClick={() => run(null, status)}>
            Remove key
          </button>
        ) : null}
      </div>
      <p className="panel-hint">
        Create a key in Linear under Settings, Account, Security &amp; access. It needs write access to tick issues off. The key is kept in the macOS Keychain and nowhere else.
      </p>
    </>
  );
}

interface Props {
  readonly value: ConnectorSettings;
  readonly onChange: (next: ConnectorSettings) => void;
}

/** Where floors can pull TODOs from: the Linear key, and how agent issues are recognised. */
export function ConnectorsGroup({ value, onChange }: Props): React.ReactElement {
  const actions = useContext(ConnectorsContext);
  const [text, setText] = useState(value.linearAgentLabel);
  const [labelError, setLabelError] = useState(false);

  useEffect(() => {
    setText(value.linearAgentLabel);
    setLabelError(false);
  }, [value.linearAgentLabel]);

  const commit = (): void => {
    const trimmed = text.trim();
    if (trimmed === "" || [...trimmed].length > AGENT_LABEL_MAX) {
      setLabelError(true);
      return;
    }
    setLabelError(false);
    if (trimmed !== value.linearAgentLabel) onChange({ ...value, linearAgentLabel: trimmed });
  };

  return (
    <>
      <h4 className="panel-subhead">Linear</h4>
      {actions === null ? null : <KeyField actions={actions} />}
      <label className="field">
        <span>Agent label</span>
        <input
          type="text"
          data-testid="linear-agent-label"
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
      </label>
      {labelError ? (
        <p className="form-error" role="alert">
          {LABEL_ERROR}
        </p>
      ) : null}
      <p className="panel-hint">Linear issues with this label go on a floor’s whiteboard, whoever they are assigned to.</p>
    </>
  );
}
