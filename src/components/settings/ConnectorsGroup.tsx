import { useContext } from "react";
import { CommitField } from "./CommitField";
import { KeyField } from "./KeyField";
import type { ConnectorSettings, LinearKeyStatus } from "../../types";
import { AGENT_LABEL_MAX } from "../../lib/agentLabel";
import { ConnectorsContext } from "../connectors/ConnectorsContext";
import type { ConnectorActions } from "../connectors/ConnectorsContext";

const LABEL_ERROR = `Enter a label of 1 to ${AGENT_LABEL_MAX} characters.`;

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

function LinearKey({ actions }: { readonly actions: ConnectorActions }): React.ReactElement {
  return (
    <KeyField
      load={actions.keyStatus}
      save={actions.setKey}
      sentence={statusSentence}
      removable={(status) => status.kind === "saved" || status.kind === "rejected"}
      busy="Checking the key with Linear…"
      placeholder="Paste a Linear API key"
      testId="linear-key"
      hint="Create a key in Linear under Settings, Account, Security & access. It needs write access to tick issues off. The key is kept in the macOS Keychain and nowhere else."
    />
  );
}

interface Props {
  readonly value: ConnectorSettings;
  readonly onChange: (next: ConnectorSettings) => void;
}

/** Where floors can pull TODOs from: the Linear key, and how agent issues are recognised. */
export function ConnectorsGroup({ value, onChange }: Props): React.ReactElement {
  const actions = useContext(ConnectorsContext);
  const commit = (text: string): string | null => {
    const trimmed = text.trim();
    if (trimmed === "" || [...trimmed].length > AGENT_LABEL_MAX) return LABEL_ERROR;
    if (trimmed !== value.linearAgentLabel) onChange({ ...value, linearAgentLabel: trimmed });
    return null;
  };

  return (
    <>
      <h4 className="panel-subhead">Linear</h4>
      {actions === null ? null : <LinearKey actions={actions} />}
      <CommitField label="Agent label" testId="linear-agent-label" value={value.linearAgentLabel} onCommit={commit} />
      <p className="panel-hint">Linear issues with this label go on a floor’s whiteboard, whoever they are assigned to.</p>
    </>
  );
}
