// "New since you looked": per session, the time you last opened it, kept in the webview's storage like the other
// per-viewer choices. Without storage the page still works; the marks then last for this run only.

import { createContext, useCallback, useMemo, useState } from "react";
import { readPref, writePref } from "../lib/prefs";
import type { Session, SessionState } from "../types";

const KEY = "office-building:seen";
/** Enough for any one person's sessions; the oldest marks go first so the entry cannot grow without end. */
const KEEP = 500;

/** Only an agent that has stopped has anything to tell you: a working one moves on every transcript record. */
const AT_REST: ReadonlySet<SessionState> = new Set<SessionState>(["idle", "waiting-human", "error", "done"]);

type Seen = Readonly<Record<string, number>>;

/** An agent at rest that moved after you last opened it (one you never opened counts as moved). */
export function hasNews(session: Session, seen: Seen): boolean {
  return AT_REST.has(session.state) && Date.parse(session.lastActivityAt) > (seen[session.id] ?? 0);
}

function load(): Seen {
  try {
    const parsed: unknown = JSON.parse(readPref(KEY) ?? "null");
    return typeof parsed === "object" && parsed !== null ? Object.fromEntries(Object.entries(parsed).filter((e): e is [string, number] => typeof e[1] === "number")) : {};
  } catch {
    return {};
  }
}

export interface SeenState {
  readonly isNew: (session: Session) => boolean;
  /** You opened this session just now. */
  readonly markSeen: (sessionId: string) => void;
}

/** What the views read: with no provider nothing is new. */
export const SeenContext = createContext<SeenState>({ isNew: () => false, markSeen: () => {} });

export function useSeen(): SeenState {
  const [seen, setSeen] = useState<Seen>(load);
  const markSeen = useCallback((sessionId: string) => {
    setSeen((s) => {
      const next = Object.fromEntries(Object.entries({ ...s, [sessionId]: Date.now() }).sort((a, b) => b[1] - a[1]).slice(0, KEEP));
      writePref(KEY, JSON.stringify(next));
      return next;
    });
  }, []);
  const isNew = useCallback((session: Session) => hasNews(session, seen), [seen]);
  return useMemo(() => ({ isNew, markSeen }), [isNew, markSeen]);
}
