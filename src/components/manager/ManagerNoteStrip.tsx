import { useContext } from "react";
import type { QueuedNote, ToolKind } from "../../types";
import { holdWords, plural } from "../../state/selectors";
import { ManagerContext } from "./ManagerContext";

const SHOWN = 140;

const cut = (text: string): string => (text.length <= SHOWN ? text : `${text.slice(0, SHOWN)}\u2026`);

/** The oldest note the manager has waiting for this desk, with the reason it waits and a way to take it back. */
export function ManagerNoteStrip({ projectId, tool, notes }: { readonly projectId: string; readonly tool: ToolKind; readonly notes: readonly QueuedNote[] }): React.ReactElement | null {
  const actions = useContext(ManagerContext);
  const [oldest, ...behind] = notes;
  if (oldest === undefined) return null;
  return (
    <div className="route-strip" data-testid="manager-note-strip">
      <span className="route-lamp" data-tone="wait" aria-hidden="true" />
      <span>Note from the manager waiting: “{cut(oldest.text)}”.</span>
      {behind.length === 0 ? null : <span>{plural(behind.length, "more behind it", "more behind it")}.</span>}
      <span>Waiting: {holdWords(oldest.hold, tool)}.</span>
      {actions === null ? null : (
        <button type="button" className="btn dark-btn" data-testid="manager-note-withdraw" onClick={() => actions.withdraw(projectId, oldest.id)}>
          Withdraw
        </button>
      )}
    </div>
  );
}
