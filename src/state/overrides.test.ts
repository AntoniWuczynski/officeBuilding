import { beforeEach, describe, expect, it } from "vitest";
import { makeDemoSnapshot } from "../api/mockData";
import { applyOverrides, NO_OVERRIDES, takeLegacyMoves, withName } from "./overrides";

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

describe("takeLegacyMoves", () => {
  const KEY = "office-building:overrides";
  beforeEach(() => localStorage.clear());

  it("hands over the moves an earlier version stored and leaves the names", () => {
    localStorage.setItem(KEY, JSON.stringify({ names: { p1: "Home base" }, moves: { s1: "p2" } }));
    expect(takeLegacyMoves()).toEqual({ s1: "p2" });
    expect(JSON.parse(localStorage.getItem(KEY) ?? "null")).toEqual({ names: { p1: "Home base" } });
    expect(takeLegacyMoves()).toEqual({});
  });

  it("forgets the whole entry when nothing else was kept in it", () => {
    localStorage.setItem(KEY, JSON.stringify({ names: {}, moves: { s1: "p2" } }));
    takeLegacyMoves();
    expect(localStorage.getItem(KEY)).toBeNull();
  });
});
