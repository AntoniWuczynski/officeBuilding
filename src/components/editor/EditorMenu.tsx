import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { Editor } from "../../types";
import type { EditorMenuItem } from "../../state/selectors";

interface Props {
  /** Where its top-left corner goes, in viewport pixels. */
  readonly at: { readonly x: number; readonly y: number };
  /** A first, non-interactive line, or none. */
  readonly caption: string | null;
  readonly items: readonly EditorMenuItem[];
  readonly onPick: (editor: Editor | null) => void;
  readonly onClose: () => void;
  /** A press inside this element does not count as outside (the chevron that toggles it). */
  readonly anchor?: HTMLElement | null;
}

const MARGIN = 8;

/** A small paper menu of editors, drawn outside the 3D scene (WebKit would slice it with the walls). */
export function EditorMenu({ at, caption, items, onPick, onClose, anchor = null }: Props): React.ReactElement {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState(at);

  // Keep it inside the window, then focus its first line.
  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    const box = el.getBoundingClientRect();
    setPos({
      x: Math.max(MARGIN, Math.min(at.x, window.innerWidth - box.width - MARGIN)),
      y: Math.max(MARGIN, Math.min(at.y, window.innerHeight - box.height - MARGIN)),
    });
    el.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [at.x, at.y]);

  useEffect(() => {
    const onDown = (e: PointerEvent): void => {
      if (!(e.target instanceof Node)) return;
      if (ref.current?.contains(e.target) === true || anchor?.contains(e.target) === true) return;
      onClose();
    };
    window.addEventListener("pointerdown", onDown, { capture: true });
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    window.addEventListener("wheel", onClose, { passive: true });
    return () => {
      window.removeEventListener("pointerdown", onDown, { capture: true });
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("wheel", onClose);
    };
  }, [anchor, onClose]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const lines = Array.from(e.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]'));
    if (lines.length === 0) return;
    const at = lines.findIndex((l) => l === document.activeElement);
    const step = e.key === "ArrowDown" ? 1 : -1;
    lines[(at + step + lines.length) % lines.length]?.focus();
  };

  return createPortal(
    <div ref={ref} className="editor-menu" role="menu" data-testid="editor-menu" style={{ left: pos.x, top: pos.y }} onKeyDown={onKeyDown}>
      {caption === null ? null : <p className="editor-menu-caption">{caption}</p>}
      {items.length === 0 ? <p className="editor-menu-empty">No editor was found on this Mac.</p> : null}
      {items.map((item) => (
        <button key={item.editor ?? "default"} type="button" role="menuitem" data-testid="editor-menu-item" data-editor={item.editor ?? ""} onClick={() => onPick(item.editor)}>
          {item.label}
        </button>
      ))}
    </div>,
    document.body,
  );
}
