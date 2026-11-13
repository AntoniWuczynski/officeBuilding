import { useCallback, useContext, useState } from "react";
import type { Place } from "../../types";
import { editorMenuItems, joinNames } from "../../state/selectors";
import { ChevronIcon } from "../icons";
import { EditorContext } from "./EditorContext";
import { EditorMenu } from "./EditorMenu";

interface Props {
  readonly place: Place;
  /** Names the place in the toast: “audit checkout”, the corner_shop floor. */
  readonly what: string;
  /** What will open, for the tooltip: "its worktree on branch x", "this floor's folder". */
  readonly opens: string;
  /** On the terminal's dark header. */
  readonly dark?: boolean;
  /** Mention Cmd+Shift+O in the tooltip. */
  readonly shortcut?: boolean;
}

/** "Open in Cursor" with a chevron for the other installed editors. Renders nothing without an EditorContext. */
export function OpenInEditor({ place, what, opens, dark = false, shortcut = false }: Props): React.ReactElement | null {
  const actions = useContext(EditorContext);
  const [menuAt, setMenuAt] = useState<{ readonly x: number; readonly y: number } | null>(null);
  const [chevron, setChevron] = useState<HTMLButtonElement | null>(null);
  const closeMenu = useCallback(() => setMenuAt(null), []);
  if (actions === null) return null;
  const { list, open } = actions;
  const items = editorMenuItems(list);
  const [first, ...others] = items;
  const title =
    first === undefined
      ? `No editor was found on this Mac. The building looks for ${joinNames((list?.editors ?? []).map((o) => o.label))}.`
      : `Open ${opens} in ${first.name}${shortcut ? " (Cmd+Shift+O)" : ""}`;
  const toggle = (e: React.MouseEvent<HTMLButtonElement>): void => {
    if (menuAt !== null) {
      setMenuAt(null);
      return;
    }
    const box = e.currentTarget.getBoundingClientRect();
    setMenuAt({ x: box.left, y: box.bottom + 2 });
  };
  return (
    <span className={dark ? "open-in is-dark" : "open-in"} data-testid="open-in">
      <button type="button" className="btn open-in-main" data-testid="open-in-main" title={title} disabled={first === undefined} onClick={() => open(place, first?.editor ?? null, what)}>
        {first === undefined ? "Open in editor" : first.label}
      </button>
      {others.length === 0 ? null : (
        <button
          ref={setChevron}
          type="button"
          className="btn open-in-more"
          data-testid="open-in-more"
          aria-label="Open in another editor"
          aria-haspopup="menu"
          aria-expanded={menuAt !== null}
          onClick={toggle}
        >
          <ChevronIcon />
        </button>
      )}
      {menuAt === null ? null : (
        <EditorMenu
          at={menuAt}
          caption={null}
          items={others}
          anchor={chevron}
          onPick={(editor) => {
            closeMenu();
            open(place, editor, what);
          }}
          onClose={closeMenu}
        />
      )}
    </span>
  );
}
