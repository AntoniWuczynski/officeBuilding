import { useContext, useState } from "react";
import type { Connection, TodoItem } from "../../types";
import { connectionLine, fileSources, joinNames, sourceBadge } from "../../state/selectors";
import { ConnectorsContext } from "../connectors/ConnectorsContext";

/** Open items shown before "Show all" (real TODO.md files can pass 2,000 items). */
const OPEN_LIMIT = 200;

interface Props {
  readonly items: readonly TodoItem[];
  readonly empty: string;
  readonly hint: string;
  readonly connection: Connection | undefined;
  readonly now: Date;
  readonly onTick: (id: string) => void;
}

export function TodoList({ items, empty, hint, connection, now, onTick }: Props): React.ReactElement {
  const [showDone, setShowDone] = useState(false);
  const [showAllOpen, setShowAllOpen] = useState(false);
  const [confirming, setConfirming] = useState<string | null>(null);
  const actions = useContext(ConnectorsContext);
  const open = items.filter((t) => !t.done);
  const done = items.filter((t) => t.done);
  const visibleOpen = showAllOpen ? open : open.slice(0, OPEN_LIMIT);
  const shown = showDone ? [...visibleOpen, ...done] : visibleOpen;
  const files = fileSources(items);
  return (
    <div className="panel-body">
      {connection === undefined ? null : (
        <p className="panel-hint connector-line" data-testid="connector-line">
          {connectionLine(connection, now)}
          {actions === null ? null : connection.needsKey ? (
            <>
              {" "}
              <button type="button" className="link" data-testid="connector-settings" onClick={actions.openSettings}>
                Open Settings
              </button>
            </>
          ) : (
            <>
              {" "}
              <button type="button" className="link" data-testid="connector-recheck" onClick={actions.recheck}>
                Re-check
              </button>
            </>
          )}
        </p>
      )}
      {items.length === 0 ? (
        <p className="panel-empty">{empty}</p>
      ) : (
        <>
          <p className="panel-hint">{files.length === 0 ? hint : `From ${joinNames(files)}. Ticking an item writes [x] into the file.`}</p>
          {open.length === 0 ? <p className="panel-empty">Nothing open.</p> : null}
          <ul className="todo-list">
            {shown.map((t) => {
              const badge = sourceBadge(t.source);
              const linear = t.source.kind === "linear" ? t.source : null;
              return (
                <li key={t.id} className={t.done ? "is-done" : undefined}>
                  <label>
                    <input
                      type="checkbox"
                      data-testid="todo-check"
                      data-todo={t.id}
                      checked={t.done}
                      disabled={t.done}
                      onChange={() => (linear === null ? onTick(t.id) : setConfirming(t.id))}
                    />
                    <span>{t.text}</span>
                  </label>
                  {badge === null ? null : linear !== null && actions !== null ? (
                    <button
                      type="button"
                      className="todo-source is-link"
                      data-testid="todo-source"
                      aria-label={`Open ${linear.identifier} in Linear`}
                      title={linear.url}
                      onClick={() => actions.openLink(t.id)}
                    >
                      {badge}
                    </button>
                  ) : (
                    <span className="todo-source" data-testid="todo-source">
                      {badge}
                    </span>
                  )}
                  {linear !== null && confirming === t.id ? (
                    <div
                      className="todo-confirm"
                      role="group"
                      aria-label={`Mark ${linear.identifier} as Done in Linear?`}
                      onKeyDown={(e) => {
                        if (e.key === "Escape") {
                          e.stopPropagation();
                          setConfirming(null);
                        }
                      }}
                    >
                      <span>Mark {linear.identifier} as Done in Linear?</span>
                      <button
                        type="button"
                        className="btn btn-primary"
                        data-testid="todo-confirm"
                        autoFocus
                        onClick={() => {
                          setConfirming(null);
                          onTick(t.id);
                        }}
                      >
                        Mark as Done
                      </button>
                      <button type="button" className="btn" data-testid="todo-cancel" onClick={() => setConfirming(null)}>
                        Cancel
                      </button>
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ul>
          {open.length > visibleOpen.length ? (
            <button type="button" className="link" onClick={() => setShowAllOpen(true)}>
              Show all {open.length} open items
            </button>
          ) : null}
          {done.length > 0 ? (
            <button type="button" className="link" data-testid="toggle-done" onClick={() => setShowDone((v) => !v)}>
              {showDone ? `Hide ${done.length} done` : `Show ${done.length} done`}
            </button>
          ) : null}
        </>
      )}
    </div>
  );
}
