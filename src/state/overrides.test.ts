import { beforeEach, describe, expect, it } from "vitest";
import { makeDemoSnapshot } from "../api/mockData";
import { act, renderHook } from "@testing-library/react";
import { applyOverrides, dropLegacyMoves, legacyMoves, NO_OVERRIDES, useOverrides, withName, withTags } from "./overrides";

const snap = makeDemoSnapshot();
const [a] = snap.projects;

describe("applyOverrides", () => {
  it("changes nothing without overrides", () => {
    expect(applyOverrides(snap, NO_OVERRIDES)).toBe(snap);
  });

  it("renames an island and nothing else", () => {
    if (a === undefined) throw new Error("demo has a project");
    const out = applyOverrides(snap, withName(NO_OVERRIDES, a.id, "Home base"));
    expect(out.projects.find((p) => p.id === a.id)?.name).toBe("Home base");
    expect(out.projects.find((p) => p.id === a.id)?.path).toBe(a.path);
    expect(out.sessions).toBe(snap.sessions);
  });

  it("a blank name takes the override away", () => {
    if (a === undefined) throw new Error("demo has a project");
    const named = withName(NO_OVERRIDES, a.id, "Home base");
    expect(withName(named, a.id, "  ")).toEqual(NO_OVERRIDES);
  });
});

describe("legacy moves", () => {
  const KEY = "office-building:overrides";
  const stored = (): unknown => JSON.parse(localStorage.getItem(KEY) ?? "null");
  beforeEach(() => localStorage.clear());

  it("are read without being taken out of storage", () => {
    localStorage.setItem(KEY, JSON.stringify({ names: { p1: "Home base" }, moves: { s1: "p2" } }));
    expect(legacyMoves()).toEqual({ s1: "p2" });
    expect(legacyMoves()).toEqual({ s1: "p2" });
  });

  it("leave storage one by one, the names staying", () => {
    localStorage.setItem(KEY, JSON.stringify({ names: { p1: "Home base" }, moves: { s1: "p2", s2: "p3" } }));
    dropLegacyMoves(["s1"]);
    expect(stored()).toEqual({ names: { p1: "Home base" }, moves: { s2: "p3" } });
    dropLegacyMoves(["s2"]);
    expect(stored()).toEqual({ names: { p1: "Home base" } });
  });

  it("forget the whole entry when nothing else was kept in it", () => {
    localStorage.setItem(KEY, JSON.stringify({ names: {}, moves: { s1: "p2" } }));
    dropLegacyMoves(["s1"]);
    expect(localStorage.getItem(KEY)).toBeNull();
  });

  it("stay in storage through a rename", () => {
    localStorage.setItem(KEY, JSON.stringify({ names: {}, moves: { s1: "p2" } }));
    const { result } = renderHook(() => useOverrides());
    act(() => result.current[1].renameFloor("p1", "Home base"));
    expect(stored()).toEqual({ names: { p1: "Home base" }, moves: { s1: "p2" } });
  });
});

describe("tags shown on an island", () => {
  const KEY = "office-building:overrides";
  const stored = (): unknown => JSON.parse(localStorage.getItem(KEY) ?? "null");
  beforeEach(() => localStorage.clear());

  it("are trimmed, kept once whatever their case, and a blank list takes them away", () => {
    const linked = withTags(NO_OVERRIDES, "p1", [" Office Building ", "office building", "", "Ops"]);
    expect(linked.tags).toEqual({ p1: ["Office Building", "Ops"] });
    expect(withTags(linked, "p1", [" "])).toEqual(NO_OVERRIDES);
  });

  it("are saved beside the names, come back after a reload, and leave a names-only entry as it was", () => {
    const { result } = renderHook(() => useOverrides());
    act(() => result.current[1].renameFloor("p1", "Home base"));
    expect(stored()).toEqual({ names: { p1: "Home base" } });
    act(() => result.current[1].linkTags("p1", ["Ops"]));
    expect(stored()).toEqual({ names: { p1: "Home base" }, tags: { p1: ["Ops"] } });
    expect(renderHook(() => useOverrides()).result.current[0].tags).toEqual({ p1: ["Ops"] });
  });

  it("do not rename anything, and keep legacy moves", () => {
    localStorage.setItem(KEY, JSON.stringify({ names: {}, moves: { s1: "p2" } }));
    const { result } = renderHook(() => useOverrides());
    act(() => result.current[1].linkTags("p1", ["Ops"]));
    expect(stored()).toEqual({ names: {}, tags: { p1: ["Ops"] }, moves: { s1: "p2" } });
    expect(applyOverrides(snap, result.current[0])).toBe(snap);
  });

  it("survive a damaged entry without throwing", () => {
    localStorage.setItem(KEY, JSON.stringify({ names: {}, tags: { p1: "Ops", p2: [3, "Ok"] } }));
    expect(renderHook(() => useOverrides()).result.current[0].tags).toEqual({ p2: ["Ok"] });
  });
});
