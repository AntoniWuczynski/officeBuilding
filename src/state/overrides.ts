// Your own say over what the app discovered: a name for a floor.
// Kept in the webview's storage (as the Office/Islands choice is) and applied once, right after a snapshot arrives,
// so every view sees the same floors. Which floor a session sits on is the backend's (`moveSession`), not kept here.

import { useCallback, useState } from "react";
import { readPref, writePref } from "../lib/prefs";
import type { OfficeSnapshot } from "../types";

export interface Overrides {
  /** Floor id to the name you gave it. */
  readonly names: Readonly<Record<string, string>>;
}

export const NO_OVERRIDES: Overrides = { names: {} };

const without = <T>(rec: Readonly<Record<string, T>>, key: string): Record<string, T> => Object.fromEntries(Object.entries(rec).filter(([k]) => k !== key));

/** A blank name takes the override away, restoring the folder's own name. */
export function withName(o: Overrides, projectId: string, name: string): Overrides {
  const trimmed = name.trim();
  return { ...o, names: trimmed === "" ? without(o.names, projectId) : { ...o.names, [projectId]: trimmed } };
}

const isEmpty = (o: Overrides): boolean => Object.keys(o.names).length === 0;

export function applyOverrides(s: OfficeSnapshot, o: Overrides): OfficeSnapshot {
  if (isEmpty(o)) return s;
  return { ...s, projects: s.projects.map((p) => ({ ...p, name: o.names[p.id] ?? p.name })) };
}

const KEY = "office-building:overrides";

const rec = (v: unknown): Record<string, string> =>
  typeof v === "object" && v !== null ? Object.fromEntries(Object.entries(v).filter((e): e is [string, string] => typeof e[1] === "string")) : {};

function stored(): { names?: unknown; moves?: unknown } {
  try {
    const parsed: unknown = JSON.parse(readPref(KEY) ?? "null");
    return typeof parsed === "object" && parsed !== null ? parsed : {};
  } catch {
    return {};
  }
}

function load(): Overrides {
  return { names: rec(stored().names) };
}

/** Moves an earlier version kept in this storage, taken out of it so they are sent to the backend once. */
export function takeLegacyMoves(): Record<string, string> {
  const moves = rec(stored().moves);
  if (Object.keys(moves).length > 0) {
    const kept = load();
    writePref(KEY, isEmpty(kept) ? null : JSON.stringify(kept));
  }
  return moves;
}

export interface OverrideActions {
  readonly renameFloor: (projectId: string, name: string) => void;
  readonly moveSession: (sessionId: string, projectId: string | null) => void;
}

/** The part of {@link OverrideActions} the webview keeps itself. */
type NameActions = Pick<OverrideActions, "renameFloor">;

export function useOverrides(): readonly [Overrides, NameActions] {
  const [overrides, setOverrides] = useState<Overrides>(load);
  const change = useCallback((next: (o: Overrides) => Overrides) => {
    setOverrides((o) => {
      const n = next(o);
      writePref(KEY, isEmpty(n) ? null : JSON.stringify(n));
      return n;
    });
  }, []);
  const renameFloor = useCallback((projectId: string, name: string) => change((o) => withName(o, projectId, name)), [change]);
  return [overrides, { renameFloor }];
}
