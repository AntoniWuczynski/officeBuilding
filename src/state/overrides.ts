// Your own say over what the app discovered: a name for a floor, and the board tags shown on it.
// Kept in the webview's storage (as the Office/Islands choice is) and applied once, right after a snapshot arrives,
// so every view sees the same floors. Which floor a session sits on is the backend's (`moveSession`), not kept here.

import { useCallback, useState } from "react";
import { entriesOf, readJsonPref, writePref } from "../lib/prefs";
import type { OfficeSnapshot } from "../types";

export interface Overrides {
  /** Floor id to the name you gave it. */
  readonly names: Readonly<Record<string, string>>;
  /** Floor id to the shared-board tags you linked to it, besides its own names. */
  readonly tags: Readonly<Record<string, readonly string[]>>;
}

const without = <T>(rec: Readonly<Record<string, T>>, key: string): Record<string, T> => Object.fromEntries(Object.entries(rec).filter(([k]) => k !== key));

/** A blank name takes the override away, restoring the folder's own name. */
export function withName(o: Overrides, projectId: string, name: string): Overrides {
  const trimmed = name.trim();
  return { ...o, names: trimmed === "" ? without(o.names, projectId) : { ...o.names, [projectId]: trimmed } };
}

/** Tags are trimmed and kept once whatever their case; no tags takes the link away. */
export function withTags(o: Overrides, projectId: string, tags: readonly string[]): Overrides {
  const kept = tags.map((t) => t.trim()).filter((t, i, all) => t !== "" && all.findIndex((u) => u.trim().toLowerCase() === t.toLowerCase()) === i);
  return { ...o, tags: kept.length === 0 ? without(o.tags, projectId) : { ...o.tags, [projectId]: kept } };
}

const isEmpty = (o: Overrides): boolean => Object.keys(o.names).length === 0 && Object.keys(o.tags).length === 0;

export function applyOverrides(s: OfficeSnapshot, o: Overrides): OfficeSnapshot {
  if (Object.keys(o.names).length === 0) return s;
  return { ...s, projects: s.projects.map((p) => ({ ...p, name: o.names[p.id] ?? p.name })) };
}

const KEY = "office-building:overrides";

const rec = (v: unknown): Record<string, string> => entriesOf(v, (x): x is string => typeof x === "string");

function stored(): { names?: unknown; tags?: unknown; moves?: unknown } {
  const parsed = readJsonPref(KEY);
  return typeof parsed === "object" && parsed !== null ? parsed : {};
}

const tagLists = (v: unknown): Record<string, string[]> => {
  const lists: Record<string, string[]> = {};
  for (const [id, list] of typeof v === "object" && v !== null ? Object.entries(v) : []) {
    const tags = Array.isArray(list) ? list.filter((t): t is string => typeof t === "string") : [];
    if (tags.length > 0) lists[id] = tags;
  }
  return lists;
};

function load(): Overrides {
  const s = stored();
  return { names: rec(s.names), tags: tagLists(s.tags) };
}

/** Moves an earlier version kept in this storage. They stay there until the backend has taken them or they can no longer apply. */
export const legacyMoves = (): Record<string, string> => rec(stored().moves);

/** Take these sessions' legacy moves out of storage, keeping the rest of it. */
export function dropLegacyMoves(sessionIds: readonly string[]): void {
  save(load(), Object.fromEntries(Object.entries(legacyMoves()).filter(([id]) => !sessionIds.includes(id))));
}

/** The names and tags, and any legacy moves not yet settled: a rename must not lose them. */
function save(o: Overrides, moves: Record<string, string>): void {
  const keepMoves = Object.keys(moves).length > 0;
  const keepTags = Object.keys(o.tags).length > 0;
  writePref(KEY, isEmpty(o) && !keepMoves ? null : JSON.stringify({ names: o.names, ...(keepTags ? { tags: o.tags } : {}), ...(keepMoves ? { moves } : {}) }));
}

export interface OverrideActions {
  readonly renameFloor: (projectId: string, name: string) => void;
  readonly moveSession: (sessionId: string, projectId: string | null) => void;
}

/** What the webview keeps itself: names (part of {@link OverrideActions}) and the tags linked to a floor. */
interface KeptActions extends Pick<OverrideActions, "renameFloor"> {
  readonly linkTags: (projectId: string, tags: readonly string[]) => void;
}

export function useOverrides(): readonly [Overrides, KeptActions] {
  const [overrides, setOverrides] = useState<Overrides>(load);
  const change = useCallback((next: (o: Overrides) => Overrides) => {
    setOverrides((o) => {
      const n = next(o);
      save(n, legacyMoves());
      return n;
    });
  }, []);
  const renameFloor = useCallback((projectId: string, name: string) => change((o) => withName(o, projectId, name)), [change]);
  const linkTags = useCallback((projectId: string, tags: readonly string[]) => change((o) => withTags(o, projectId, tags)), [change]);
  return [overrides, { renameFloor, linkTags }];
}
