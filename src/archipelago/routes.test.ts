import { describe, expect, it } from "vitest";
import type { Project } from "../types";
import { layoutIslands } from "./islands";
import { bezierAt, sailRoute } from "./routes";

const many = (n: number): Project[] => Array.from({ length: n }, (_, i) => ({ id: `id-${i}`, name: `repo-${i}`, path: `/work/f/repo-${i}`, sessionIds: [] }));

describe("sailRoute", () => {
  it("runs shore to shore and clears every other island's land, for every pair in a 30-island cluster", () => {
    const islands = layoutIslands(many(30), "none");
    let crossings = 0;
    for (const a of islands) {
      for (const b of islands) {
        if (a === b) continue;
        const route = sailRoute(a, b, islands);
        expect(Math.hypot(route.from.x - a.x, route.from.z - a.z)).toBeGreaterThan(a.radius);
        expect(Math.hypot(route.to.x - b.x, route.to.z - b.z)).toBeGreaterThan(b.radius);
        for (let s = 0; s <= 24; s++) {
          const at = bezierAt(route, s / 24);
          for (const o of islands) {
            if (o !== a && o !== b && Math.hypot(at.x - o.x, at.z - o.z) <= o.radius) crossings++;
          }
        }
      }
    }
    expect(crossings).toBe(0);
  });

  it("sails straight when nothing is in the way", () => {
    const [a, b] = layoutIslands(many(2), "none");
    if (a === undefined || b === undefined) throw new Error("two islands");
    const route = sailRoute(a, b, [a, b]);
    const mid = bezierAt(route, 0.5);
    expect(mid.x).toBeCloseTo((route.from.x + route.to.x) / 2, 5);
    expect(mid.z).toBeCloseTo((route.from.z + route.to.z) / 2, 5);
  });
});
