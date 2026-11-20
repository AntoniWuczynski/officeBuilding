import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { makeSeedSnapshot } from "../api/mockData";
import type { Session, SessionState } from "../types";
import { hasNews, useSeen } from "./useSeen";

const KEY = "office-building:seen";
const base = makeSeedSnapshot().sessions[0] as Session;
const at = (state: SessionState, lastActivityAt: string): Session => ({ ...base, id: "x", state, lastActivityAt });

describe("hasNews", () => {
  const seen = { x: Date.parse("2026-09-28T12:00:00Z") };

  it("is true for an agent at rest that moved after you looked", () => {
    for (const state of ["idle", "waiting-human", "error", "done"] as const) expect(hasNews(at(state, "2026-09-28T12:00:01Z"), seen)).toBe(true);
  });

  it("is never true for a working agent, however recently it moved", () => {
    for (const state of ["running", "thinking"] as const) expect(hasNews(at(state, "2026-09-28T13:00:00Z"), seen)).toBe(false);
  });

  it("is false when nothing moved since you looked, and true for an agent you never opened", () => {
    expect(hasNews(at("idle", "2026-09-28T12:00:00Z"), seen)).toBe(false);
    expect(hasNews(at("idle", "2026-09-28T11:00:00Z"), seen)).toBe(false);
    expect(hasNews(at("idle", "2026-09-28T11:00:00Z"), {})).toBe(true);
  });
});

describe("useSeen", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-09-29T09:00:00Z"));
  });
  afterEach(() => vi.useRealTimers());

  it("clears an agent's news when it is opened, and keeps that across a reload", () => {
    localStorage.setItem("office-building:seen-since", String(Date.parse("2026-09-28T00:00:00Z")));
    const idle = at("idle", "2026-09-28T12:00:00Z");
    const { result } = renderHook(() => useSeen());
    expect(result.current.isNew(idle)).toBe(true);
    act(() => result.current.markSeen("x"));
    expect(result.current.isNew(idle)).toBe(false);
    expect(renderHook(() => useSeen()).result.current.isNew(idle)).toBe(false);
    expect(result.current.isNew({ ...idle, lastActivityAt: "2026-09-29T09:00:01Z" })).toBe(true);
  });

  it("works without storage, and with a damaged entry", () => {
    localStorage.setItem(KEY, "{not json");
    localStorage.setItem("office-building:seen-since", String(Date.parse("2026-09-28T00:00:00Z")));
    const { result } = renderHook(() => useSeen());
    expect(result.current.isNew(at("idle", "2026-09-28T12:00:00Z"))).toBe(true);
    const broken = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    act(() => result.current.markSeen("x"));
    expect(result.current.isNew(at("idle", "2026-09-28T12:00:00Z"))).toBe(false);
    broken.mockRestore();
  });

  it("keeps the 500 newest marks, so the entry cannot grow without end", () => {
    const { result } = renderHook(() => useSeen());
    for (let i = 0; i < 501; i++) {
      vi.setSystemTime(new Date(Date.parse("2026-09-29T09:00:00Z") + i * 1000));
      act(() => result.current.markSeen(`s${i}`));
    }
    const kept = Object.keys(JSON.parse(localStorage.getItem(KEY) ?? "{}") as object);
    expect(kept).toHaveLength(500);
    expect(kept).not.toContain("s0");
  });
});

describe("useSeen on a first launch", () => {
  const SINCE_KEY = "office-building:seen-since";
  const before = "2026-09-28T12:00:00Z";
  const after = "2026-09-29T09:00:01Z";
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-09-29T09:00:00Z"));
  });
  afterEach(() => vi.useRealTimers());

  it("treats what happened before the first launch as seen, and what moves after it as new", () => {
    const { result } = renderHook(() => useSeen());
    expect(result.current.isNew(at("idle", before))).toBe(false);
    expect(result.current.isNew(at("idle", after))).toBe(true);
    expect(result.current.isNew(at("running", after))).toBe(false);
  });

  it("keeps the first launch across a reload, writing it only once", () => {
    renderHook(() => useSeen());
    vi.setSystemTime(new Date("2026-09-30T09:00:00Z"));
    const write = vi.spyOn(Storage.prototype, "setItem");
    const { result } = renderHook(() => useSeen());
    expect(write).not.toHaveBeenCalled();
    expect(localStorage.getItem(SINCE_KEY)).toBe(String(Date.parse("2026-09-29T09:00:00Z")));
    expect(result.current.isNew(at("idle", "2026-09-29T12:00:00Z"))).toBe(true);
    write.mockRestore();
  });

  it("marks an opened agent new only when it moves again", () => {
    const { result } = renderHook(() => useSeen());
    vi.setSystemTime(new Date("2026-09-29T10:00:00Z"));
    act(() => result.current.markSeen("x"));
    expect(result.current.isNew(at("idle", after))).toBe(false);
    expect(result.current.isNew(at("idle", "2026-09-29T10:00:01Z"))).toBe(true);
  });

  it("without storage, counts from the start of this run", () => {
    const broken = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    const { result } = renderHook(() => useSeen());
    expect(result.current.isNew(at("idle", before))).toBe(false);
    expect(result.current.isNew(at("idle", after))).toBe(true);
    broken.mockRestore();
  });

  it("writes the marks once per markSeen even when React runs updaters twice", () => {
    const { result } = renderHook(() => useSeen(), { wrapper: StrictMode });
    const write = vi.spyOn(Storage.prototype, "setItem");
    act(() => result.current.markSeen("x"));
    expect(write.mock.calls.filter(([key]) => key === KEY)).toHaveLength(1);
    write.mockRestore();
  });
});
