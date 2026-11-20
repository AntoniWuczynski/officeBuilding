import { useEffect, useRef, type KeyboardEvent as ReactKeyboardEvent } from "react";

/** The layers open now, oldest first: one Escape closes only the newest. */
const opened: symbol[] = [];

/**
 * While `open`, Escape closes this layer if it is the newest and nothing nearer the focus (a dialog, a field) handled the key first.
 * However the layer closes, the focus it took away goes back to what held it when the layer opened.
 */
export function useEscapeClose(open: boolean, close: () => void): void {
  // Held in a ref so a fresh `close` each render does not re-register the layer and move it to the top of the stack.
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    if (!open) return;
    // Layer effects run before the opener's own, so this is still what opened it (a nameplate, a toggle).
    const opener = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null;
    const me = Symbol("layer");
    opened.push(me);
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== "Escape" || e.defaultPrevented || opened[opened.length - 1] !== me) return;
      e.preventDefault();
      closeRef.current();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      opened.splice(opened.indexOf(me), 1);
      // Only focus left with nowhere to go (its element removed) goes back; a deliberate move elsewhere stays.
      const lost = document.activeElement === null || document.activeElement === document.body;
      if (lost && opener?.isConnected === true) opener.focus();
    };
  }, [open]);
}

/** A field's `onKeyDown`: a first Escape drops what was typed, handled there, so the layer's Escape waits for the next press. */
export function dropDraftOnEscape(draft: string, drop: () => void): (e: ReactKeyboardEvent) => void {
  return (e) => {
    if (e.key !== "Escape" || draft === "") return;
    e.preventDefault();
    drop();
  };
}
