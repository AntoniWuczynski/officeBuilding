import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";
import type { AgentMessage } from "../types";
import { useNewMessages } from "./useNewMessages";

const msg = (id: string): AgentMessage => ({ id, fromSessionId: "a", toSessionId: "b", text: id, sentAt: "2026-09-28T15:00:00Z" });

describe("useNewMessages", () => {
  it("hands over a message that arrives after mount once, and never one that was already there", () => {
    const { result, rerender } = renderHook(({ ms }) => useNewMessages(ms), { initialProps: { ms: [msg("old")] } });
    expect(result.current()).toEqual([]);
    rerender({ ms: [msg("old"), msg("new")] });
    expect(result.current().map((m) => m.id)).toEqual(["new"]);
    expect(result.current()).toEqual([]);
  });

  it("keeps the same function across renders, so an animation frame can hold it", () => {
    const { result, rerender } = renderHook(({ ms }) => useNewMessages(ms), { initialProps: { ms: [msg("a")] } });
    const first = result.current;
    rerender({ ms: [msg("a"), msg("b")] });
    expect(result.current).toBe(first);
  });
});
