// A boat's course between two islands. Pure, so it is tested without WebGL.

import type { IslandSpec, Slot } from "./islands";

export interface Route {
  readonly from: Slot;
  /** Quadratic bezier control point: bends the course round islands that lie between. */
  readonly via: Slot;
  readonly to: Slot;
}

/** Shore clearance as a multiple of the radius: the coastline swings up to 23% out (build.ts blob). */
const SHORE = 1.35;
/** Detours tried in order, as world units off the straight line at the middle of the course. */
const DETOURS = [0, ...Array.from({ length: 16 }, (_, i) => (i + 1) * 25).flatMap((d) => [d, -d])];
const SAMPLES = 24;

/** The point on an island's shore facing a point of the sea. */
function shoreToward(from: IslandSpec, x: number, z: number): Slot {
  const d = Math.hypot(x - from.x, z - from.z) || 1;
  return { x: from.x + ((x - from.x) / d) * from.radius * SHORE, z: from.z + ((z - from.z) / d) * from.radius * SHORE };
}

export function bezierAt(r: Route, k: number): Slot {
  const u = 1 - k;
  return { x: u * u * r.from.x + 2 * u * k * r.via.x + k * k * r.to.x, z: u * u * r.from.z + 2 * u * k * r.via.z + k * k * r.to.z };
}

/**
 * The course from one island's shore to another's, bent as little as it needs to be to stay clear of every other
 * island. When none of the tried detours clears them all, the one that crosses fewest islands is taken.
 */
export function sailRoute(a: IslandSpec, b: IslandSpec, all: readonly IslandSpec[]): Route {
  const from = shoreToward(a, b.x, b.z);
  const to = shoreToward(b, a.x, a.z);
  const len = Math.hypot(to.x - from.x, to.z - from.z) || 1;
  const nx = -(to.z - from.z) / len;
  const nz = (to.x - from.x) / len;
  const others = all.filter((i) => i.projectId !== a.projectId && i.projectId !== b.projectId);
  let best: { readonly route: Route; readonly hits: number } | null = null;
  for (const detour of DETOURS) {
    // A bezier's midpoint sits half way to its control point, so the control point goes out twice the detour.
    const route: Route = { from, to, via: { x: (from.x + to.x) / 2 + nx * detour * 2, z: (from.z + to.z) / 2 + nz * detour * 2 } };
    let hits = 0;
    for (let s = 0; s <= SAMPLES; s++) {
      const at = bezierAt(route, s / SAMPLES);
      if (others.some((o) => Math.hypot(at.x - o.x, at.z - o.z) < o.radius * SHORE)) hits++;
    }
    if (hits === 0) return route;
    if (best === null || hits < best.hits) best = { route, hits };
  }
  return best?.route ?? { from, to, via: { x: (from.x + to.x) / 2, z: (from.z + to.z) / 2 } };
}
