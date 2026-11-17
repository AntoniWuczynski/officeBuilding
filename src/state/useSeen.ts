// "New since you looked": per session, the time you last opened it, kept in the webview's storage like the other
// per-viewer choices. Without storage the page still works; the marks then last for this run only.

import { createContext, useCallback, useMemo, useRef, useState } from "react";
import { readPref, writePref } from "../lib/prefs";
import type { Session, SessionState } from "../types";

const KEY = "office-building:seen";
const SINCE_KEY = "office-building:seen-since";
/** Enough for any one person's sessions; the oldest marks go first so the entry cannot grow without end. */
const KEEP = 500;

/** Only an agent that has stopped has anything to tell you: a working one moves on every transcript record. */
const AT_REST: ReadonlySet<SessionState> = new Set<SessionState>(["idle", "waiting-human", "error", "done"]);

type Seen = Readonly<Record<string, number>>;

/** An agent at rest that moved after you last opened it; one you never opened counts from `since`, the first launch. */
export function hasNews(session: Session, seen: Seen, since = 0): boolean {
  return AT_REST.has(session.state) && Date.parse(session.lastActivityAt) > (seen[session.id] ?? since);
}

/** The first launch, so the history already there is not news. Without storage it is this run's start. */
function loadSince(): number {
  const stored = readPref(SINCE_KEY);
  if (stored !== null && Number.isFinite(Number(stored))) return Number(stored);
  const now = Date.now();
  writePref(SINCE_KEY, String(now));
  return now;
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
  const [since] = useState(loadSince);
  const latest = useRef(seen);
  const markSeen = useCallback((sessionId: string) => {
    const next = Object.fromEntries(Object.entries({ ...latest.current, [sessionId]: Date.now() }).sort((a, b) => b[1] - a[1]).slice(0, KEEP));
    latest.current = next;
    setSeen(next);
    writePref(KEY, JSON.stringify(next));
  }, []);
  const isNew = useCallback((session: Session) => hasNews(session, seen, since), [seen, since]);
  return useMemo(() => ({ isNew, markSeen }), [isNew, markSeen]);
}
