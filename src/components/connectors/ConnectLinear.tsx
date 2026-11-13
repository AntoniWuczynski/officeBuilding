import { useContext, useEffect, useState } from "react";
import type { LinearKeyStatus, LinearLink, LinearTeam } from "../../types";
import { message } from "../../lib/errors";
import { ConnectorsContext } from "./ConnectorsContext";
import type { ConnectorActions } from "./ConnectorsContext";

interface Props {
  readonly projectId: string;
  readonly current: LinearLink | null;
  readonly onDone: () => void;
}

type Load<T> = { readonly phase: "reading" } | { readonly phase: "failed"; readonly message: string } | { readonly phase: "ready"; readonly value: T };

function keySentence(status: Exclude<LinearKeyStatus, { kind: "saved" }>): string {
  switch (status.kind) {
    case "missing":
      return "Add a Linear API key in Settings first.";
    case "rejected":
      return "Linear did not accept the saved key. Replace it in Settings.";
    case "unavailable":
      return `The Keychain did not hand over the key: ${status.reason}.`;
  }
}

/** Key status first, then the team list, then the form. */
export function ConnectLinear({ projectId, current, onDone }: Props): React.ReactElement | null {
  const actions = useContext(ConnectorsContext);
  if (actions === null) return null;
  return <KeyGate actions={actions} projectId={projectId} current={current} onDone={onDone} />;
}

function KeyGate({ actions, projectId, current, onDone }: Props & { readonly actions: ConnectorActions }): React.ReactElement {
  const [key, setKey] = useState<Load<LinearKeyStatus>>({ phase: "reading" });

  useEffect(() => {
    let live = true;
    actions.keyStatus().then(
      (value) => live && setKey({ phase: "ready", value }),
      (err: unknown) => live && setKey({ phase: "failed", message: message(err) }),
    );
    return () => {
      live = false;
    };
  }, [actions]);

  if (key.phase === "reading") {
    return (
      <p className="panel-hint" role="status">
        Reading the key’s status…
      </p>
    );
  }
  if (key.phase === "failed") {
    return (
      <div className="linear-form">
        <p className="form-error" role="alert">
          Could not read the key’s status: {key.message}.
        </p>
        <Cancel onDone={onDone} />
      </div>
    );
  }
  if (key.value.kind !== "saved") {
    return (
      <div className="linear-form">
        <p className="panel-hint">{keySentence(key.value)}</p>
        <div className="linear-actions">
          <button type="button" className="btn btn-primary" data-testid="linear-open-settings" onClick={actions.openSettings}>
            Open Settings
          </button>
          <Cancel onDone={onDone} />
        </div>
      </div>
    );
  }
  return <TeamGate actions={actions} projectId={projectId} current={current} onDone={onDone} />;
}

function Cancel({ onDone }: { readonly onDone: () => void }): React.ReactElement {
  return (
    <button type="button" className="btn" data-testid="linear-cancel" onClick={onDone}>
      Cancel
    </button>
  );
}

function TeamGate({ actions, projectId, current, onDone }: Props & { readonly actions: ConnectorActions }): React.ReactElement {
  const [teams, setTeams] = useState<Load<readonly LinearTeam[]>>({ phase: "reading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setTeams({ phase: "reading" });
    actions.teams().then(
      (value) => live && setTeams({ phase: "ready", value }),
      (err: unknown) => live && setTeams({ phase: "failed", message: message(err) }),
    );
    return () => {
      live = false;
    };
  }, [actions, attempt]);

  if (teams.phase === "reading") {
    return (
      <p className="panel-hint" role="status">
        Reading your Linear teams…
      </p>
    );
  }
  if (teams.phase === "failed") {
    return (
      <div className="linear-form">
        <p className="form-error" role="alert">
          Could not read your Linear teams: {teams.message}.
        </p>
        <div className="linear-actions">
          <button type="button" className="link" data-testid="linear-retry" onClick={() => setAttempt(attempt + 1)}>
            Try again
          </button>
          <Cancel onDone={onDone} />
        </div>
      </div>
    );
  }
  if (teams.value.length === 0) {
    return (
      <div className="linear-form">
        <p className="panel-hint">This key cannot see any Linear team.</p>
        <Cancel onDone={onDone} />
      </div>
    );
  }
  return <LinearForm actions={actions} teams={teams.value} projectId={projectId} current={current} onDone={onDone} />;
}

function LinearForm({ actions, teams, projectId, current, onDone }: Props & { readonly actions: ConnectorActions; readonly teams: readonly LinearTeam[] }): React.ReactElement {
  const startTeam = teams.find((t) => t.id === current?.teamId);
  const [teamId, setTeamId] = useState(startTeam?.id ?? "");
  const [projectChoice, setProjectChoice] = useState(startTeam === undefined ? "" : (current?.project?.id ?? ""));
  const [label, setLabel] = useState(current?.label ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const team = teams.find((t) => t.id === teamId);
  const stored = team?.id === current?.teamId && current?.project?.id === projectChoice ? current.project : null;
  const chosenProject = team?.projects.find((p) => p.id === projectChoice) ?? stored;

  const submit = (): void => {
    if (team === undefined) return;
    setSaving(true);
    setError(null);
    const trimmed = label.trim();
    actions
      .connect(projectId, { kind: "linear", teamId: team.id, teamKey: team.key, teamName: team.name, project: chosenProject, label: trimmed === "" ? null : trimmed })
      .then(onDone, (err: unknown) => {
        setSaving(false);
        setError(`Could not connect: ${message(err)}.`);
      });
  };

  return (
    <div className="linear-form">
      <label className="field">
        <span>Team</span>
        <select
          data-testid="linear-team"
          value={teamId}
          onChange={(e) => {
            setTeamId(e.target.value);
            setProjectChoice("");
          }}
        >
          <option value="">Choose a team</option>
          {teams.map((t) => (
            <option key={t.id} value={t.id}>
              {`${t.key} · ${t.name}`}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>Project</span>
        <select data-testid="linear-project" value={projectChoice} onChange={(e) => setProjectChoice(e.target.value)}>
          <option value="">Any project</option>
          {stored !== null && !(team?.projects ?? []).some((p) => p.id === stored.id) ? (
            <option value={stored.id}>{`${stored.name} (closed)`}</option>
          ) : null}
          {(team?.projects ?? []).map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>Label</span>
        <input data-testid="linear-label" placeholder="Any label" value={label} onChange={(e) => setLabel(e.target.value)} />
      </label>
      <p className="panel-hint">
        Issues assigned to you go on the cork board. Unassigned issues, and issues with the agent label from Settings, go on the whiteboard. Only issues that are not started or in progress are shown.
      </p>
      {error === null ? null : (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      <div className="linear-actions">
        <button type="button" className="btn btn-primary" data-testid="linear-submit" disabled={team === undefined || saving} onClick={submit}>
          Connect
        </button>
        <Cancel onDone={onDone} />
      </div>
    </div>
  );
}
