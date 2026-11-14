import { useCallback, useState } from "react";
import { readPref, writePref } from "../lib/prefs";

export type ViewMode = "office" | "islands";
const STORAGE_KEY = "office-building:view";

/** Which overview shows: the office building or the island map. The last one picked, kept in the webview's storage; the office until one is. */
export function useViewMode(): readonly [ViewMode, (mode: ViewMode) => void] {
  const [mode, setMode] = useState<ViewMode>(() => (readPref(STORAGE_KEY) === "islands" ? "islands" : "office"));
  const choose = useCallback((next: ViewMode) => {
    setMode(next);
    writePref(STORAGE_KEY, next);
  }, []);
  return [mode, choose];
}
