import { useEffect } from "react";

/** The popovers open now, oldest first: one Escape closes only the newest. */
const opened: symbol[] = [];

/** While `open`, Escape closes this popover first, and is marked handled so a panel under it keeps its own Escape. */
export function useEscapeClose(open: boolean, close: () => void): void {
  useEffect(() => {
    if (!open) return;
    const me = Symbol("popover");
    opened.push(me);
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== "Escape" || opened[opened.length - 1] !== me) return;
      e.preventDefault();
      close();
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => {
      window.removeEventListener("keydown", onKey, { capture: true });
      opened.splice(opened.indexOf(me), 1);
    };
  }, [open, close]);
}
