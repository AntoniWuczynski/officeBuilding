import { describe, expect, it } from "vitest";
import type { Project } from "../types";
import { agentSlots, assignSeats, assignSlots, clampToBounds, closestPair, CLUSTER_STEP, layoutClusters, layoutIslands, MAX_SLOTS, mergeOrder, shortName, worldBounds } from "./islands";
import type { SlotAt } from "./islands";

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

function many(n: number): Project[] {
  return Array.from({ length: n }, (_, i) => p(`id-${i}`, `repo-${i}`, `/work/folder-${i % 3}/repo-${i}`));
}

describe("layoutIslands", () => {
  it("leaves every placed island where it was when a project is added", () => {
    const before = layoutIslands(PROJECTS, "8");
    const after = layoutIslands([...PROJECTS, p("10", "elsewhere", "/work/elsewhere/elsewhere")], "8");
    for (const island of before) expect(after.find((x) => x.projectId === island.projectId)).toEqual(island);
  });

  it("makes a cluster: islands from different folders spread both ways, not along one line", () => {
    const islands = layoutIslands(many(12), "none");
    const xs = islands.map((i) => i.x);
    const zs = islands.map((i) => i.z);
    const spanX = Math.max(...xs) - Math.min(...xs);
    const spanZ = Math.max(...zs) - Math.min(...zs);
    expect(spanZ / spanX).toBeGreaterThan(0.6);
    expect(spanX / spanZ).toBeGreaterThan(0.6);
  });

  it("makes the setup project the lighthouse", () => {
    expect(layoutIslands(PROJECTS, "8").find((i) => i.projectId === "8")?.theme).toBe("lighthouse");
  });

  it("leaves open sea between every pair of islands, from 2 to 60 projects", () => {
    for (let n = 2; n <= 60; n++) {
      expect(closestPair(layoutIslands(many(n), "none")), `${n} projects`).toBeGreaterThan(4);
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

describe("agentSlots spread", () => {
  it("spreads a few agents evenly round the island instead of bunching them on one side", () => {
    for (const spec of layoutIslands(PROJECTS, "8")) {
      for (let n = 2; n <= MAX_SLOTS; n++) {
        const slots = agentSlots(spec, n);
        expect(slots).toHaveLength(n);
        let nearest = Infinity;
        for (let i = 0; i < n; i++)
          for (let j = i + 1; j < n; j++) {
            const a = slots[i];
            const b = slots[j];
            if (a !== undefined && b !== undefined) nearest = Math.min(nearest, Math.hypot(a.x - b.x, a.z - b.z));
          }
        // Evenly spaced on the ring: the chord between neighbours.
        const ring = spec.radius * 0.58;
        expect(nearest, `${n} agents on ${spec.name}`).toBeCloseTo(2 * ring * Math.sin(Math.PI / n), 5);
      }
    }
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

describe("assignSeats", () => {
  const spec = layoutIslands(PROJECTS, "8")[0];
  const ids = (n: number): string[] => Array.from({ length: n }, (_, i) => `s${i + 1}`);
  const where = (seats: ReadonlyMap<string, number>): Map<string, { x: number; z: number }> => {
    const ring = agentSlots(spec as NonNullable<typeof spec>);
    return new Map([...seats].map(([id, seat]) => [id, ring[seat] as { x: number; z: number }]));
  };

  it("keeps every other agent's seat when a 7th arrives and the 2nd leaves", () => {
    const six = assignSeats(new Map(), ids(6));
    const before = where(six);
    const after = where(assignSeats(six, [...ids(7).filter((id) => id !== "s2")]));
    for (const id of ["s1", "s3", "s4", "s5", "s6"]) expect(after.get(id), id).toEqual(before.get(id));
    expect(after.has("s7")).toBe(true);
  });

  it("gives the newcomer the first free seat, which is the one the leaver left when it comes first", () => {
    const six = assignSeats(new Map(), ids(6));
    const next = assignSeats(six, [...ids(6).filter((id) => id !== "s2"), "s7"]);
    expect(next.get("s7")).toBe(six.get("s2"));
  });

  it("seats at most MAX_SLOTS agents, each on its own seat, and seats a waiting agent once one frees", () => {
    const full = assignSeats(new Map(), ids(MAX_SLOTS + 2));
    expect(full.size).toBe(MAX_SLOTS);
    expect(new Set(full.values()).size).toBe(MAX_SLOTS);
    const freed = assignSeats(full, ids(MAX_SLOTS + 2).filter((id) => id !== "s1"));
    expect(freed.size).toBe(MAX_SLOTS);
    expect(freed.has("s9")).toBe(true);
  });

  it("spreads the first agents round the ring: two sit opposite, four sit a quarter apart", () => {
    const seats = (n: number): number[] => [...assignSeats(new Map(), ids(n)).values()].sort((a, b) => a - b);
    expect(seats(2)).toEqual([0, 4]);
    expect(seats(4)).toEqual([0, 2, 4, 6]);
  });
});

describe("shortName", () => {
  it("leaves a short title alone and cuts a long one with an ellipsis", () => {
    expect(shortName("audit checkout")).toBe("audit checkout");
    expect(shortName("orders ingest fix for the new queue")).toBe("orders ingest…");
    expect(shortName("  hi  ")).toBe("hi");
  });
  it("names a session with a blank title rather than leaving an empty tag", () => {
    expect(shortName("   ")).toBe("Agent");
  });
});

describe("layoutClusters", () => {
  const ids = (n: number, tag: string): string[] => Array.from({ length: n }, (_, i) => `${tag}${i}`);
  /** `sizes[c]` repos in cluster `c`, laid out as Archipelago does: order by first sight, slots by assignSlots. */
  function build(sizes: readonly number[], prev: ReadonlyMap<string, SlotAt> = new Map()) {
    const groups = new Map(sizes.map((n, c) => [`c${c}`, ids(n, `r${c}-`)] as const));
    const slots = assignSlots(prev, groups);
    const projects = [...groups.values()].flat().map((id) => p(id, id, `/work/${id}`));
    return { slots, ...layoutClusters([...groups.keys()], slots, projects, "none"), projects };
  }
  const at = (l: { islands: { projectId: string; x: number; z: number }[] }, id: string) => l.islands.find((i) => i.projectId === id);

  it("keeps 92 units between any two islands for 1..12 clusters of 1..8 islands", () => {
    for (let c = 1; c <= 12; c++)
      for (let n = 1; n <= 8; n++) {
        const { islands } = build(Array<number>(c).fill(n));
        for (const a of islands) for (const b of islands) if (a !== b) expect(Math.hypot(a.x - b.x, a.z - b.z), `${c} x ${n}`).toBeGreaterThanOrEqual(91.9);
        expect(closestPair(islands), `${c} x ${n}`).toBeGreaterThan(4);
      }
  });

  it("moves no existing island when a cluster is added or one grows from 1 to 8", () => {
    const before = build([2, 1, 3]);
    expect(build([2, 1, 3, 4]).islands.slice(0, 6).map((i) => [i.x, i.z])).toEqual(before.islands.map((i) => [i.x, i.z]));
    let prev = build([1, 2, 2]);
    for (let n = 2; n <= 8; n++) {
      const next = build([n, 2, 2], prev.slots);
      for (const i of prev.islands) expect(at(next, i.projectId), `${n}`).toMatchObject({ x: i.x, z: i.z });
      prev = next;
    }
  });

  it("puts clusters 9..16 on an outer ring and moves none of the first 8", () => {
    const eight = build(Array<number>(8).fill(1));
    const sixteen = build(Array<number>(16).fill(1));
    for (const i of eight.islands) expect(at(sixteen, i.projectId)).toMatchObject({ x: i.x, z: i.z });
    expect(closestPair(sixteen.islands)).toBeGreaterThan(4);
  });

  it("moves no island outside a cluster when the first guess makes a new one", () => {
    const ungrouped = new Map([["ungrouped", ["a", "b", "c"]]]);
    const slots = assignSlots(new Map(), ungrouped);
    const all = ["a", "b", "c"].map((id) => p(id, id, `/work/${id}`));
    const before = layoutClusters(["ungrouped"], slots, all, "none").islands;
    const moved = assignSlots(slots, new Map([["ungrouped", ["a", "b"]], ["beta", ["c"]]]));
    const after = layoutClusters(["ungrouped", "beta"], moved, all, "none").islands;
    expect(after.filter((i) => i.projectId !== "c").map((i) => [i.x, i.z])).toEqual(before.filter((i) => i.projectId !== "c").map((i) => [i.x, i.z]));
    expect(after.find((i) => i.projectId === "c")).not.toMatchObject({ x: before[2]?.x, z: before[2]?.z });
  });

  it("gives a repo that changes cluster the next free slot of its new one and leaves the others", () => {
    const first = assignSlots(new Map(), new Map([["u", ["a", "b"]], ["alpha", ["c"]]]));
    const next = assignSlots(first, new Map([["u", ["b"]], ["alpha", ["c", "a"]]]));
    expect(next.get("a")).toEqual({ cluster: "alpha", slot: 1 });
    expect(next.get("b")).toEqual(first.get("b"));
    expect(next.get("c")).toEqual(first.get("c"));
  });

  it("does not draw an empty cluster and keeps the lighthouse out of the rings", () => {
    const projects = [p("a", "a", "/w/a"), p("s", "Setup", "/w/s")];
    const l = layoutClusters(["gone", "alpha"], assignSlots(new Map(), new Map([["alpha", ["a"]]])), projects, "s");
    expect([...l.centres.keys()]).toEqual(["alpha"]);
    const lh = l.islands.find((i) => i.projectId === "s");
    if (lh === undefined) throw new Error("no lighthouse");
    expect(lh.theme).toBe("lighthouse");
    for (const i of l.islands) if (i !== lh) expect(Math.hypot(i.x - lh.x, i.z - lh.z)).toBeGreaterThan(92);
  });
  it("keeps the lighthouse one step out with one cluster, and clear of every island with 23 or more", () => {
    const one = layoutClusters(["c0"], assignSlots(new Map(), new Map([["c0", ["a"]]])), [p("a", "a", "/w/a"), p("s", "Setup", "/w/s")], "s");
    const near = one.islands.find((i) => i.projectId === "s");
    if (near === undefined) throw new Error("no lighthouse");
    expect(Math.hypot(near.x, near.z)).toBeLessThanOrEqual(2 * CLUSTER_STEP);
    for (const n of [23, 30, 40]) {
      const groups = new Map(Array.from({ length: n }, (_, c) => [`c${c}`, [`r${c}`]] as const));
      const projects = [...groups.values()].flat().map((id) => p(id, id, `/w/${id}`)).concat(p("s", "Setup", "/w/s"));
      const l = layoutClusters([...groups.keys()], assignSlots(new Map(), groups), projects, "s");
      const house = l.islands.find((i) => i.projectId === "s");
      if (house === undefined) throw new Error("no lighthouse");
      for (const i of l.islands) if (i !== house) expect(Math.hypot(i.x - house.x, i.z - house.z), `${n}`).toBeGreaterThan(92);
    }
  });
});
