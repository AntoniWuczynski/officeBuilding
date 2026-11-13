import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { makeSettings } from "../api/mockSettings";
import type { Settings, SettingsPatch } from "../types";
import { useSettings } from "./useSettings";

const saved = makeSettings();
const quiet = (s: Settings): Settings => ({ ...s, notifications: { ...s.notifications, sound: false } });

function deferred<T>(): { promise: Promise<T>; resolve: (v: T) => void; reject: (e: Error) => void } {
  let resolve: (v: T) => void = () => undefined;
  let reject: (e: Error) => void = () => undefined;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("useSettings", () => {
  it("loads, then is ready", async () => {
    const load = vi.fn(() => Promise.resolve(saved));
    const { result } = renderHook(() => useSettings(load, vi.fn()));
    expect(result.current.state.status).toBe("loading");
    await waitFor(() => expect(result.current.state).toEqual({ status: "ready", settings: saved, saveError: null }));
  });

  it("shows a failed load and loads again on retry", async () => {
    const load = vi.fn<() => Promise<Settings>>().mockRejectedValueOnce(new Error("no file")).mockResolvedValue(saved);
    const { result } = renderHook(() => useSettings(load, vi.fn()));
    await waitFor(() => expect(result.current.state).toEqual({ status: "error", message: "no file" }));
    act(() => result.current.retry());
    expect(result.current.state.status).toBe("loading");
    await waitFor(() => expect(result.current.state.status).toBe("ready"));
  });

  it("shows a change at once and settles on what the backend returned", async () => {
    const load = vi.fn(() => Promise.resolve(saved));
    const reply = deferred<Settings>();
    const save = vi.fn((_patch: SettingsPatch) => reply.promise);
    const { result } = renderHook(() => useSettings(load, save));
    await waitFor(() => expect(result.current.state.status).toBe("ready"));
    const patch = { notifications: { ...saved.notifications, sound: false } };
    act(() => result.current.change(patch));
    expect(save).toHaveBeenCalledWith(patch);
    expect(result.current.state).toEqual({ status: "ready", settings: { ...saved, ...patch }, saveError: null });
    const back = { ...saved, notifications: { ...patch.notifications, cooldownSecs: 30 } };
    await act(async () => reply.resolve(back));
    expect(result.current.state).toEqual({ status: "ready", settings: back, saveError: null });
  });

  it("reports a failed save and reloads the saved values", async () => {
    const load = vi.fn(() => Promise.resolve(saved));
    const save = vi.fn((_patch: SettingsPatch) => Promise.reject(new Error("disk full")));
    const { result } = renderHook(() => useSettings(load, save));
    await waitFor(() => expect(result.current.state.status).toBe("ready"));
    act(() => result.current.change(quiet(saved)));
    await waitFor(() => expect(result.current.state).toEqual({ status: "ready", settings: saved, saveError: "disk full" }));
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("keeps the error and the shown values when the reload fails too", async () => {
    const load = vi.fn<() => Promise<Settings>>().mockResolvedValueOnce(saved).mockRejectedValue(new Error("gone"));
    const save = vi.fn((_patch: SettingsPatch) => Promise.reject(new Error("disk full")));
    const { result } = renderHook(() => useSettings(load, save));
    await waitFor(() => expect(result.current.state.status).toBe("ready"));
    act(() => result.current.change(quiet(saved)));
    await waitFor(() => expect(result.current.state).toEqual({ status: "ready", settings: quiet(saved), saveError: "disk full" }));
  });

  it("applies only the newest request's outcome", async () => {
    const load = vi.fn(() => Promise.resolve(saved));
    const first = deferred<Settings>();
    const second = deferred<Settings>();
    const save = vi.fn<(patch: SettingsPatch) => Promise<Settings>>().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { result } = renderHook(() => useSettings(load, save));
    await waitFor(() => expect(result.current.state.status).toBe("ready"));
    const newer = { ...saved, notifications: { ...saved.notifications, stuck: false } };
    act(() => result.current.change(quiet(saved)));
    act(() => result.current.change(newer));
    await act(async () => second.resolve(newer));
    await act(async () => first.reject(new Error("late failure")));
    expect(result.current.state).toEqual({ status: "ready", settings: newer, saveError: null });
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("does nothing while loading", () => {
    const load = vi.fn(() => new Promise<Settings>(() => undefined));
    const save = vi.fn();
    const { result } = renderHook(() => useSettings(load, save));
    act(() => result.current.change(quiet(saved)));
    expect(save).not.toHaveBeenCalled();
    expect(result.current.state.status).toBe("loading");
  });

  it("ignores a load that arrives after unmount", async () => {
    const reply = deferred<Settings>();
    const { result, unmount } = renderHook(() => useSettings(() => reply.promise, vi.fn()));
    unmount();
    await act(async () => reply.resolve(saved));
    expect(result.current.state.status).toBe("loading");
  });
});
