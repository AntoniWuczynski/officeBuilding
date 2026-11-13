import { describe, expect, it } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { useLoad } from "./useLoad";

describe("useLoad", () => {
  it("reads, then is ready", async () => {
    const load = (): Promise<number> => Promise.resolve(3);
    const { result } = renderHook(() => useLoad(load));
    expect(result.current.state).toEqual({ phase: "reading" });
    await waitFor(() => expect(result.current.state).toEqual({ phase: "ready", value: 3 }));
  });

  it("fails with the message", async () => {
    const load = (): Promise<number> => Promise.reject(new Error("boom"));
    const { result } = renderHook(() => useLoad(load));
    await waitFor(() => expect(result.current.state).toEqual({ phase: "failed", message: "boom" }));
  });

  it("set replaces the value", async () => {
    const load = (): Promise<number> => Promise.resolve(1);
    const { result } = renderHook(() => useLoad(load));
    await waitFor(() => expect(result.current.state.phase).toBe("ready"));
    act(() => result.current.set(9));
    expect(result.current.state).toEqual({ phase: "ready", value: 9 });
  });

  it("drops a result that arrives after load changed", async () => {
    let finishFirst: (value: string) => void = () => {};
    const first = (): Promise<string> => new Promise((resolve) => (finishFirst = resolve));
    const second = (): Promise<string> => Promise.resolve("second");
    const { result, rerender } = renderHook(({ load }) => useLoad(load), { initialProps: { load: first } });
    rerender({ load: second });
    await waitFor(() => expect(result.current.state).toEqual({ phase: "ready", value: "second" }));
    await act(async () => {
      finishFirst("first");
      await Promise.resolve();
    });
    expect(result.current.state).toEqual({ phase: "ready", value: "second" });
  });

  it("loads again when load changes", async () => {
    const one = (): Promise<string> => Promise.resolve("one");
    const two = (): Promise<string> => Promise.resolve("two");
    const { result, rerender } = renderHook(({ load }) => useLoad(load), { initialProps: { load: one } });
    await waitFor(() => expect(result.current.state).toEqual({ phase: "ready", value: "one" }));
    rerender({ load: two });
    await waitFor(() => expect(result.current.state).toEqual({ phase: "ready", value: "two" }));
  });
});
