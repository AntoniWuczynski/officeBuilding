import { useCallback, useState } from "react";
import { readPref, writePref } from "../lib/prefs";

export const DEFAULT_BUILDING_NAME = "Office Building";
const STORAGE_KEY = "office-building:name";
export const MAX_BUILDING_NAME = 40;

/** The name on the roof sign: the human's own, kept in the webview's storage. A blank name restores the default. */
export function useBuildingName(): readonly [string, (name: string) => void] {
  const [name, setName] = useState(() => readPref(STORAGE_KEY) ?? DEFAULT_BUILDING_NAME);
  const rename = useCallback((next: string) => {
    const trimmed = next.trim().slice(0, MAX_BUILDING_NAME);
    const value = trimmed === "" ? DEFAULT_BUILDING_NAME : trimmed;
    setName(value);
    writePref(STORAGE_KEY, value === DEFAULT_BUILDING_NAME ? null : value);
  }, []);
  return [name, rename];
}
