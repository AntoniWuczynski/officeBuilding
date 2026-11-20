import { useState } from "react";
import { useProjects } from "../../state/ProjectsContext";
import type { SessionTag } from "../../state/clusters";
import { message } from "../../lib/errors";
import type { ProjectChoice, ProjectTarget } from "../../api/DiscoveryApi";
import { dropDraftOnEscape } from "./useEscapeClose";

interface Props {
  /** Accessible name of the select, e.g. "Project" or "Project for beta-web". */
  readonly label: string;
  /** What the select shows now, or null when nothing is chosen yet. */
  readonly current: string | null;
  readonly target: ProjectTarget;
  /** A name to rank the suggestions against (a session's name); the typed name wins once typing. */
  readonly hint?: string;
  /** Called after a choice was saved. */
  readonly onAssigned?: () => void;
}

const NEW = "\u0000new";
const optionKey = (c: ProjectChoice, i: number): string => `${i}:${c.id ?? c.name}`;

/** The words a session's tag shows, the same in every view: a project name, or "Harness, <home>". Unplaced shows none. */
export function tagText(tag: SessionTag | undefined): string | null {
  if (tag === undefined || tag.kind === "unplaced") return null;
  return tag.kind === "harness" && tag.home !== null ? `${tag.label} · ${tag.home}` : tag.label;
}

/** Existing projects: the rules file's, then guessed ones. A guess is chosen by its key, a rule by its id. */
function useChoices(): ProjectChoice[] {
  const { rules, clusters } = useProjects();
  const out: ProjectChoice[] = rules.map((r) => ({ id: r.id, name: r.name }));
  for (const c of clusters) if (c.kind === "project" && c.source === "guess" && c.id !== null) out.push({ id: c.id, name: c.label });
  return out;
}

/** A project picker for a repo or a session: existing names (similar ones first), or a new name typed in. One assignProject per choice. */
export function ProjectPick({ label, current, target, hint, onAssigned }: Props): React.ReactElement {
  const { assignProject, rulesError, similar } = useProjects();
  const choices = useChoices();
  const [creating, setCreating] = useState(false);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const disabled = rulesError !== null;
  const names = choices.map((c) => c.name);
  const ranked = (input: string): ProjectChoice[] => {
    const near = similar(input, names);
    const rank = (c: ProjectChoice): number => {
      const i = near.indexOf(c.name);
      return i < 0 ? near.length : i;
    };
    return [...choices].sort((a, b) => rank(a) - rank(b));
  };
  const choose = (p: ProjectChoice): void => {
    setError(null);
    assignProject(target, p).then(
      () => {
        setCreating(false);
        setText("");
        onAssigned?.();
      },
      (err: unknown) => setError(message(err)),
    );
  };
  const typed = text.trim();
  return (
    <span className="place-project-pick">
      <select
        className="place-move"
        aria-label={label}
        value=""
        disabled={disabled}
        onChange={(e) => {
          if (e.target.value === NEW) setCreating(true);
          else {
            const picked = ranked(hint ?? "").find((c, i) => optionKey(c, i) === e.target.value);
            if (picked !== undefined) choose(picked);
          }
        }}
      >
        <option value="">{current ?? "Project..."}</option>
        {ranked(hint ?? "").map((c, i) => (
          <option key={optionKey(c, i)} value={optionKey(c, i)}>
            {c.name}
          </option>
        ))}
        <option value={NEW}>New project</option>
      </select>
      {creating && !disabled ? (
        <span className="place-project-new">
          <input aria-label="New project" value={text} placeholder="Project name" onChange={(e) => setText(e.target.value)} onKeyDown={dropDraftOnEscape(text, () => setText(""))} />
          {typed === ""
            ? null
            : ranked(typed).filter((c) => similar(typed, names).includes(c.name)).map((c) => (
                <button key={c.id ?? c.name} type="button" className="btn" onClick={() => choose(c)}>
                  {c.name}
                </button>
              ))}
          {typed === "" ? null : (
            <button type="button" className="btn" onClick={() => choose({ id: null, name: typed })}>
              Create {typed}
            </button>
          )}
        </span>
      ) : null}
      {disabled || error !== null ? (
        <span className="form-error" role="alert">
          {disabled ? rulesError : error}
        </span>
      ) : null}
    </span>
  );
}

/** The place panel's Project field: which project's cluster this repo sits in. */
export function ProjectField({ repoKey, current }: { readonly repoKey: string; readonly current: string | null }): React.ReactElement {
  return (
    <div className="place-project" data-testid="place-project">
      <span>Project</span>
      <ProjectPick label="Project" current={current} target={{ kind: "repo", key: repoKey }} />
    </div>
  );
}
