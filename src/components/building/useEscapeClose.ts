import { useEffect, useRef } from "react";

/** The layers open now, oldest first: one Escape closes only the newest. */
const opened: symbol[] = [];

/** While `open`, Escape closes this layer if it is the newest and nothing nearer the focus (a dialog, a field) handled the key first. */
export function useEscapeClose(open: boolean, close: () => void): void {
  // Held in a ref so a fresh `close` each render does not re-register the layer and move it to the top of the stack.
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    if (!open) return;
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
    };
  }, [open]);
}
