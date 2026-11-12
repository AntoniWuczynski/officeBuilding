import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { EditorList } from "../types";
import { useEditors } from "./useEditors";

const list = (defaultEditor: EditorList["defaultEditor"]): EditorList => ({
  editors: [{ editor: "cursor", label: "Cursor", installed: true }],
  defaultEditor,
});

function deferred<T>(): { promise: Promise<T>; resolve: (v: T) => void } {
  let resolve: (v: T) => void = () => undefined;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("useEditors", () => {
  it("loads on mount", async () => {
    const load = vi.fn(() => Promise.resolve(list("cursor")));
    const { result } = renderHook(() => useEditors(load));
    expect(result.current.list).toBeNull();
    await waitFor(() => expect(result.current.list).toEqual(list("cursor")));
  });

  it("replaces the list on reload", async () => {
    const load = vi.fn<() => Promise<EditorList>>().mockResolvedValueOnce(list("cursor")).mockResolvedValue(list(null));
    const { result } = renderHook(() => useEditors(load));
    await waitFor(() => expect(result.current.list).toEqual(list("cursor")));
    act(() => result.current.reload());
    await waitFor(() => expect(result.current.list).toEqual(list(null)));
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("drops an older answer that arrives late", async () => {
    const first = deferred<EditorList>();
    const second = deferred<EditorList>();
    const load = vi.fn<() => Promise<EditorList>>().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { result } = renderHook(() => useEditors(load));
    act(() => result.current.reload());
    await act(async () => {
      second.resolve(list(null));
      await second.promise;
    });
    await act(async () => {
      first.resolve(list("cursor"));
      await first.promise;
    });
    expect(result.current.list).toEqual(list(null));
  });

  it("keeps the previous list when a load fails", async () => {
    const load = vi.fn<() => Promise<EditorList>>().mockResolvedValueOnce(list("cursor")).mockRejectedValue(new Error("no shell"));
    const { result } = renderHook(() => useEditors(load));
    await waitFor(() => expect(result.current.list).toEqual(list("cursor")));
    act(() => result.current.reload());
    await waitFor(() => expect(load).toHaveBeenCalledTimes(2));
    await act(() => Promise.resolve());
    expect(result.current.list).toEqual(list("cursor"));
  });
});
