import { useCallback, useEffect, useRef, useState } from "react";

/** On screen since first paint, new since then, or gone from the data but still animating out. */
export type Phase = "here" | "arriving" | "leaving";

export interface Present<T> {
  readonly key: string;
  readonly item: T;
  readonly phase: Phase;
}

interface Gone<T> {
  readonly key: string;
  readonly item: T;
  /** Where it stood, so the rest do not close the gap until it has left. */
  readonly index: number;
}

/**
 * `items` plus the ones that just left, kept at their old place (as last seen)
 * until `done(key)` says their exit has played. Without `animate` they go at once.
 */
export function usePresence<T>(
  items: readonly T[],
  keyOf: (item: T) => string,
  animate: boolean,
): { readonly list: readonly Present<T>[]; readonly done: (key: string) => void } {
  const [initial] = useState(() => new Set(items.map(keyOf)));
  // The items as last committed: what a leaver looked like before it went.
  const last = useRef(items);
  useEffect(() => {
    last.current = items;
  });
  const keys = items.map(keyOf);
  const signature = keys.join("\u0000");
  const [seen, setSeen] = useState(signature);
  const [gone, setGone] = useState<readonly Gone<T>[]>([]);
  if (signature !== seen) {
    setSeen(signature);
    const now = new Set(keys);
    // One that leaves and later returns arrives afresh.
    for (const key of initial) if (!now.has(key)) initial.delete(key);
    const left = animate
      ? last.current.flatMap((item, index) => (now.has(keyOf(item)) ? [] : [{ key: keyOf(item), item, index }]))
      : [];
    setGone((current) => [...current.filter((g) => !now.has(g.key) && !left.some((l) => l.key === g.key)), ...left]);
  }
  const done = useCallback((key: string) => setGone((current) => current.filter((g) => g.key !== key)), []);

  const list: Present<T>[] = items.map((item, i) => ({ key: keys[i] ?? "", item, phase: initial.has(keys[i] ?? "") ? "here" : "arriving" }));
  for (const g of [...gone].sort((a, b) => a.index - b.index)) {
    if (keys.includes(g.key)) continue;
    list.splice(Math.min(g.index, list.length), 0, { key: g.key, item: g.item, phase: "leaving" });
  }
  return { list, done };
}
