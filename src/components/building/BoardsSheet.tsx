import { useCallback } from "react";
import type { SharedBoard } from "../../types";
import { openCount } from "../../state/selectors";
import { useNow } from "../../state/useNow";
import { TodoList } from "../floor/TodoList";
import { useEscapeClose } from "./useEscapeClose";
import "./boards.css";

interface Props {
  readonly boards: readonly SharedBoard[];
  readonly onTick: (todoId: string) => void;
  /** Where the files are chosen; the empty sheet points there. */
  readonly onOpenSettings?: () => void;
  /** The view keeps it, so only one top-bar sheet is open at a time. */
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
}

/** A top-bar button, in both views, opening each shared checklist file's items, where they can be ticked. */
export function BoardsSheet({ boards, onTick, onOpenSettings, open, onOpenChange: setOpen }: Props): React.ReactElement {
  const now = useNow();
  const waiting = boards.reduce((n, b) => n + openCount(b.items), 0);
  const close = useCallback(() => setOpen(false), [setOpen]);
  useEscapeClose(open, close);
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
