import { describe, expect, it } from "vitest";
import type { Project } from "../types";
import { agentSlots, clampToBounds, closestPair, layoutIslands, MAX_SLOTS, mergeOrder, worldBounds } from "./islands";

const p = (id: string, name: string, path: string): Project => ({ id, name, path, sessionIds: [] });

const PROJECTS: Project[] = [
  p("1", "web-app", "/work/code/web-app"),
  p("2", "api-server", "/work/code/api-server"),
  p("3", "research-notes", "/work/code/research-notes"),
  p("4", "docs", "/work/code/docs"),
  p("5", "garden", "/work/side/garden"),
  p("6", "recipes", "/work/side/recipes"),
  p("8", "Setup", "/work/Library/Application Support/app/Setup"),
];

/** n projects spread round-robin over `groups` folders. */
function many(n: number, groups: number): Project[] {
  return Array.from({ length: n }, (_, i) => p(`id-${i}`, `repo-${i}`, `/work/folder-${i % groups}/repo-${i}`));
}

describe("layoutIslands", () => {
  it("leaves every placed island where it was when a new folder appears", () => {
    const before = layoutIslands(PROJECTS, "8");
    const after = layoutIslands([...PROJECTS, p("10", "elsewhere", "/work/elsewhere/elsewhere")], "8");
    for (const island of before) expect(after.find((x) => x.projectId === island.projectId)).toEqual(island);
  });

  it("leaves every placed island where it was when a repo joins a folder that still has room", () => {
    // Five repos reserve room for eight: a sixth fits.
    const five = [...many(5, 1), p("b", "b", "/work/other/b")];
    const before = layoutIslands(five, "none");
    const after = layoutIslands([...five, p("id-5", "repo-5", "/work/folder-0/repo-5")], "none");
    for (const island of before) expect(after.find((x) => x.projectId === island.projectId)).toEqual(island);
  });

  it("moves only the groups after one that outgrows its reserved room", () => {
    const eight = [...many(8, 1), p("b", "b", "/work/other/b")];
    const before = layoutIslands(eight, "none");
    const after = layoutIslands([...many(9, 1), p("b", "b", "/work/other/b")], "none");
    for (const island of before.filter((i) => i.group === "/work/folder-0")) {
      expect(after.find((x) => x.projectId === island.projectId)).toEqual(island);
    }
    expect(after.find((x) => x.projectId === "b")?.x).toBeGreaterThan(before.find((x) => x.projectId === "b")?.x ?? 0);
  });

  it("groups repos by the folder they live in", () => {
    const islands = layoutIslands(PROJECTS, "8");
    const groupOf = (id: string): string | undefined => islands.find((i) => i.projectId === id)?.group;
    expect(groupOf("1")).toBe(groupOf("2"));
    expect(groupOf("5")).toBe(groupOf("6"));
    expect(groupOf("1")).not.toBe(groupOf("5"));
    expect(islands.find((i) => i.projectId === "8")?.theme).toBe("lighthouse");
  });

  it("leaves open sea between every pair of islands, from 1 to 40 projects in one, two or three folders", () => {
    for (const groups of [1, 2, 3]) {
      for (let n = 1; n <= 40; n++) {
        const gap = closestPair(layoutIslands(many(n, groups), "none"));
        if (n > 1) expect(gap, `${n} projects in ${groups} folders`).toBeGreaterThan(4);
      }
    }
  });
});

describe("mergeOrder", () => {
  it("keeps placed projects in place, appends new ones, and never drops a removed one", () => {
    expect(mergeOrder(["a", "b", "c"], ["a", "c", "d"])).toEqual(["a", "b", "c", "d"]);
    expect(mergeOrder([], ["x", "y"])).toEqual(["x", "y"]);
  });

  it("so removing an island leaves the others where they were", () => {
    const all = layoutIslands(PROJECTS, "8");
    // The removed project stays in the layout as a hole: everything else keeps its place.
    const order = mergeOrder(PROJECTS.map((x) => x.id), PROJECTS.filter((x) => x.id !== "2").map((x) => x.id));
    const known = new Map(PROJECTS.map((x) => [x.id, x]));
    const after = layoutIslands(order.flatMap((id) => known.get(id) ?? []), "8").filter((i) => i.projectId !== "2");
    for (const island of after) expect(island).toEqual(all.find((x) => x.projectId === island.projectId));
  });
});

describe("agentSlots", () => {
  it("seats every agent on the grass, apart from each other", () => {
    for (const spec of layoutIslands(PROJECTS, "8")) {
      const slots = agentSlots(spec);
      expect(slots).toHaveLength(MAX_SLOTS);
      for (const s of slots) expect(Math.hypot(s.x - spec.x, s.z - spec.z)).toBeLessThan(spec.radius * 0.7);
      for (let i = 1; i < slots.length; i++) {
        const a = slots[i - 1];
        const b = slots[i];
        if (a !== undefined && b !== undefined) expect(Math.hypot(a.x - b.x, a.z - b.z)).toBeGreaterThan(2.5);
      }
    }
  });
});

describe("clampToBounds", () => {
  it("keeps the camera target inside the archipelago plus open sea", () => {
    const b = worldBounds(layoutIslands(PROJECTS, "8"), 20);
    expect(clampToBounds(-9999, 9999, b)).toEqual({ x: b.minX, z: b.maxZ });
    expect(clampToBounds(9999, -9999, b)).toEqual({ x: b.maxX, z: b.minZ });
    const mid = { x: (b.minX + b.maxX) / 2, z: (b.minZ + b.maxZ) / 2 };
    expect(clampToBounds(mid.x, mid.z, b)).toEqual(mid);
  });
});
