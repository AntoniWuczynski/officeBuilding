import { describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { usePresence } from "./usePresence";

const key = (s: string): string => s;
const phases = (list: readonly { readonly key: string; readonly phase: string }[]): string[] => list.map((p) => `${p.key}:${p.phase}`);

describe("usePresence", () => {
  it("keeps a leaver at its old place until its exit has played, and marks newcomers", () => {
    const { result, rerender } = renderHook(({ items }) => usePresence(items, key, true), { initialProps: { items: ["a", "b", "c"] } });
    expect(phases(result.current.list)).toEqual(["a:here", "b:here", "c:here"]);
    rerender({ items: ["a", "c", "d"] });
    expect(phases(result.current.list)).toEqual(["a:here", "b:leaving", "c:here", "d:arriving"]);
    // A re-render with the same keys (a fresh array) changes nothing.
    rerender({ items: ["a", "c", "d"] });
    expect(phases(result.current.list)).toEqual(["a:here", "b:leaving", "c:here", "d:arriving"]);
    act(() => result.current.done("b"));
    expect(phases(result.current.list)).toEqual(["a:here", "c:here", "d:arriving"]);
  });

  it("lets one that comes back arrive afresh, and drops leavers at once without animation", () => {
    const { result, rerender } = renderHook(({ items, animate }) => usePresence(items, key, animate), {
      initialProps: { items: ["a", "b"], animate: true },
    });
    rerender({ items: ["a"], animate: true });
    rerender({ items: ["a", "b"], animate: true });
    expect(phases(result.current.list)).toEqual(["a:here", "b:arriving"]);
    rerender({ items: ["b"], animate: false });
    expect(phases(result.current.list)).toEqual(["b:arriving"]);
  });
});
