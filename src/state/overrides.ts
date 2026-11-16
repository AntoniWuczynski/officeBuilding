// Your own say over what the app discovered: a name for a floor, and which floor a session sits on.
// Kept in the webview's storage (as the Office/Islands choice is) and applied once, right after a snapshot arrives,
// so every view sees the same floors.

import { useCallback, useState } from "react";
import { readPref, writePref } from "../lib/prefs";
import type { OfficeSnapshot, Session } from "../types";

export interface Overrides {
  /** Floor id to the name you gave it. */
  readonly names: Readonly<Record<string, string>>;
  /** Session id to the floor you put it on. */
  readonly moves: Readonly<Record<string, string>>;
}

export const NO_OVERRIDES: Overrides = { names: {}, moves: {} };

const without = <T>(rec: Readonly<Record<string, T>>, key: string): Record<string, T> => Object.fromEntries(Object.entries(rec).filter(([k]) => k !== key));

/** A blank name takes the override away, restoring the folder's own name. */
export function withName(o: Overrides, projectId: string, name: string): Overrides {
  const trimmed = name.trim();
  return { ...o, names: trimmed === "" ? without(o.names, projectId) : { ...o.names, [projectId]: trimmed } };
}

/** `null` puts the session back on the floor discovery chose. */
export function withMove(o: Overrides, sessionId: string, projectId: string | null): Overrides {
  return { ...o, moves: projectId === null ? without(o.moves, sessionId) : { ...o.moves, [sessionId]: projectId } };
}

const isEmpty = (o: Overrides): boolean => Object.keys(o.names).length === 0 && Object.keys(o.moves).length === 0;

export function applyOverrides(s: OfficeSnapshot, o: Overrides): OfficeSnapshot {
  if (isEmpty(o)) return s;
  const known = new Set(s.projects.map((p) => p.id));
  const destOf = (x: Session): string => {
    const to = o.moves[x.id];
    return to !== undefined && known.has(to) ? to : x.projectId;
  };
  const moved = new Map<string, string>();
  for (const x of s.sessions) if (destOf(x) !== x.projectId) moved.set(x.id, destOf(x));
  const sessions = moved.size === 0 ? s.sessions : s.sessions.map((x) => (moved.has(x.id) ? { ...x, projectId: destOf(x) } : x));
  const projects = s.projects.map((p) => ({
    ...p,
    name: o.names[p.id] ?? p.name,
    sessionIds: moved.size === 0 ? p.sessionIds : [...p.sessionIds.filter((id) => !moved.has(id)), ...[...moved].filter(([, to]) => to === p.id).map(([id]) => id)],
  }));
  const questions = moved.size === 0 ? s.questions : s.questions.map((q) => (moved.has(q.sessionId) ? { ...q, projectId: moved.get(q.sessionId) ?? q.projectId } : q));
  return { ...s, projects, sessions, questions };
}

const KEY = "office-building:overrides";

function load(): Overrides {
  try {
    const parsed: unknown = JSON.parse(readPref(KEY) ?? "null");
    if (typeof parsed !== "object" || parsed === null) return NO_OVERRIDES;
    const rec = (v: unknown): Record<string, string> =>
      typeof v === "object" && v !== null ? Object.fromEntries(Object.entries(v).filter((e): e is [string, string] => typeof e[1] === "string")) : {};
    const p = parsed as { names?: unknown; moves?: unknown };
    return { names: rec(p.names), moves: rec(p.moves) };
  } catch {
    return NO_OVERRIDES;
  }
}

export interface OverrideActions {
  readonly renameFloor: (projectId: string, name: string) => void;
  readonly moveSession: (sessionId: string, projectId: string | null) => void;
}

export function useOverrides(): readonly [Overrides, OverrideActions] {
  const [overrides, setOverrides] = useState<Overrides>(load);
  const change = useCallback((next: (o: Overrides) => Overrides) => {
    setOverrides((o) => {
      const n = next(o);
      writePref(KEY, isEmpty(n) ? null : JSON.stringify(n));
      return n;
    });
  }, []);
  const renameFloor = useCallback((projectId: string, name: string) => change((o) => withName(o, projectId, name)), [change]);
  const moveSession = useCallback((sessionId: string, projectId: string | null) => change((o) => withMove(o, sessionId, projectId)), [change]);
  return [overrides, { renameFloor, moveSession }];
}
