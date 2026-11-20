import { useCallback, useRef } from "react";
import type { AgentMessage } from "../types";

/**
 * Messages that reached the snapshot after this view mounted, each handed over once. A floor's walking notes and the
 * island map's boats and parrots both draw only these: a message already there on open would otherwise replay.
 * Returned as a function so an animation frame can ask as well as an effect.
 */
export function useNewMessages(messages: readonly AgentMessage[]): () => readonly AgentMessage[] {
  const seen = useRef(new Set(messages.map((m) => m.id)));
  const latest = useRef(messages);
  latest.current = messages;
  return useCallback(() => {
    const fresh = latest.current.filter((m) => !seen.current.has(m.id));
    for (const m of fresh) seen.current.add(m.id);
    return fresh;
  }, []);
}
