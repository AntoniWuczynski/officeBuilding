import type { BoardSettings } from "../../types";
import { CommitField } from "./CommitField";
import "../building/boards.css";

interface Props {
  readonly value: BoardSettings;
  readonly onChange: (next: BoardSettings) => void;
}

/** The files shown on the Boards sheet: add one by its full path, remove one with its button. */
export function BoardsGroup({ value, onChange }: Props): React.ReactElement {
  const add = (text: string): string | null => {
    const path = text.trim();
    if (path === "") return null;
    if (!path.startsWith("/")) return "Use the file's full path, starting with /.";
    if (value.sharedFiles.includes(path)) return "That file is already on the list.";
    onChange({ sharedFiles: [...value.sharedFiles, path] });
    return null;
  };
  return (
    <>
      <p className="panel-hint">Checklist files that show on the Boards button in both views. Ticking an item writes [x] into the file. A line tagged with a project name also shows on that project&apos;s Human TODO.</p>
      {value.sharedFiles.length === 0 ? null : (
        <ul className="boards-files" data-testid="settings-boards">
          {value.sharedFiles.map((path) => (
            <li key={path}>
              <span>{path}</span>
              <button type="button" className="btn" aria-label={`Remove ${path}`} onClick={() => onChange({ sharedFiles: value.sharedFiles.filter((p) => p !== path) })}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      {/* Keyed by the list so a file that was added leaves the field empty. */}
      <CommitField key={value.sharedFiles.length} label="Add a file" testId="settings-boards-add" value="" placeholder="/Users/you/Desktop/MY-ACTIONS.md" onCommit={add} />
    </>
  );
}
