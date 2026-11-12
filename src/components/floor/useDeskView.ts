import { useState } from "react";
import type { DeskView } from "../../types";

/** The view each desk was last left in, kept while the app runs. */
const remembered = new Map<string, DeskView>();

/** A desk's view (chat or terminal): it starts at `initial` and stays where it was put for the rest of the run. */
export function useDeskView(sessionId: string, initial: DeskView): readonly [DeskView, (next: DeskView) => void] {
  const [view, setView] = useState<DeskView>(() => remembered.get(sessionId) ?? initial);
  return [
    view,
    (next) => {
      remembered.set(sessionId, next);
      setView(next);
    },
  ];
}
