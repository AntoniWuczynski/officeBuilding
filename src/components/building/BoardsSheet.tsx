import { useEffect, useState } from "react";
import type { SharedBoard } from "../../types";
import { openCount } from "../../state/selectors";
import { useNow } from "../../state/useNow";
import { TodoList } from "../floor/TodoList";
import "./boards.css";

interface Props {
  readonly boards: readonly SharedBoard[];
  readonly onTick: (todoId: string) => void;
  /** Where the files are chosen; the empty sheet points there. */
  readonly onOpenSettings?: () => void;
}

/** A top-bar button, in both views, opening each shared checklist file's items, where they can be ticked. */
export function BoardsSheet({ boards, onTick, onOpenSettings }: Props): React.ReactElement {
  const [open, setOpen] = useState(false);
  const now = useNow();
  const waiting = boards.reduce((n, b) => n + openCount(b.items), 0);
  useEffect(() => {
    if (!open) return;
    // Captured and marked handled, as the Needs-you list does, so a panel under the sheet keeps this Escape.
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      setOpen(false);
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, [open]);
  return (
    <span className="boards-menu">
      <button type="button" className="btn" data-testid="boards" aria-expanded={open} aria-controls="boards-sheet" onClick={() => setOpen(!open)}>
        Boards{waiting > 0 ? ` ${waiting}` : ""}
      </button>
      {open ? (
        <section id="boards-sheet" className="boards-sheet" aria-label="Shared boards">
          {boards.length === 0 ? (
            <p className="boards-empty">
              No shared files yet.{" "}
              {onOpenSettings === undefined ? (
                "Add one under Boards in Settings."
              ) : (
                <button
                  type="button"
                  className="link"
                  onClick={() => {
                    setOpen(false);
                    onOpenSettings();
                  }}
                >
                  Open Settings
                </button>
              )}
            </p>
          ) : (
            boards.map((b) => (
              <section key={b.id} className="board" aria-label={b.name} data-testid="board">
                <h3>{b.name}</h3>
                <TodoList items={b.items} empty="Nothing on this board." hint="Tick an item off when you have done it." connection={undefined} now={now} onTick={onTick} />
              </section>
            ))
          )}
        </section>
      ) : null}
    </span>
  );
}
