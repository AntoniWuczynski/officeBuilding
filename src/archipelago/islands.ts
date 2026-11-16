// Where each project's island sits and what it is built of.
// Pure and deterministic so a project keeps its island between launches and tests need no WebGL.

import { hashString as hash } from "../lib/looks";
import type { Project } from "../types";

export type IslandTheme = "workshop" | "observatory" | "control" | "farm" | "lighthouse";

export interface IslandSpec {
  readonly projectId: string;
  readonly name: string;
  readonly x: number;
  readonly z: number;
  /** Island radius in world units, before coastline noise. */
  readonly radius: number;
  readonly theme: IslandTheme;
  /** Seeds the coastline, so the shape is stable per project. */
  readonly seed: number;
}

/** Name hints first (a research repo gets an observatory); otherwise a stable pick by hash. */
const HINTS: readonly (readonly [RegExp, IslandTheme])[] = [
  [/research|science|lab|notebook|study/i, "observatory"],
  [/ops|infra|deploy|server|api|backend|platform/i, "control"],
];
const FALLBACK: readonly IslandTheme[] = ["workshop", "observatory", "control", "farm"];

function themeFor(project: Project): IslandTheme {
  for (const [re, theme] of HINTS) if (re.test(project.name)) return theme;
  return FALLBACK[hash(project.id) % FALLBACK.length] ?? "workshop";
}

const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));
/**
 * Spiral constant: on r = SPACING * sqrt(i) no two islands sit closer than SPACING (islands.test.ts measures 2 to 60).
 * Radii run 22..26 and the coastline swings up to 23% out (build.ts blob), so an island reaches 32 and 92 leaves at
 * least 92 - 2 * 32 = 28 units of open sea.
 */
const SPACING = 92;

/** Projects in a stable order: ones already placed keep their place, new ones go last. Removed ones are never dropped. */
export function mergeOrder(order: readonly string[], currentIds: readonly string[]): string[] {
  const known = new Set(order);
  return [...order, ...currentIds.filter((id) => !known.has(id))];
}

/**
 * One golden-angle spiral of islands around the origin, in the order the projects were first seen: a cluster, not a
 * row. A new repo takes the next place on the spiral, so no island already placed moves. The setup project becomes
 * the lighthouse islet.
 */
export function layoutIslands(projects: readonly Project[], setupProjectId: string): IslandSpec[] {
  return projects.map((project, i) => {
    const a = i * GOLDEN_ANGLE;
    const r = i === 0 ? 0 : SPACING * Math.sqrt(i);
    const isSetup = project.id === setupProjectId;
    return {
      projectId: project.id,
      name: project.name,
      x: Math.cos(a) * r,
      z: Math.sin(a) * r,
      radius: isSetup ? 11 : 22 + (hash(project.id) % 5),
      theme: isSetup ? "lighthouse" : themeFor(project),
      seed: hash(project.id),
    };
  });
}

/** Seats per island; the rest of a crowded island shows as a "+N" on its label. */
export const MAX_SLOTS = 8;
/** Ring radius as a share of the island radius: inside the grass (0.76) and clear of the hill (0.4). */
const SLOT_RING = 0.58;

export interface Slot {
  readonly x: number;
  readonly z: number;
}

/**
 * World positions of `count` seats spaced evenly on a ring, offset by the island's seed so islands do not look alike.
 * The island itself uses the full ring and assignSeats (a seat must not move when another agent comes or goes);
 * a smaller `count` spreads that many evenly.
 */
export function agentSlots(spec: IslandSpec, count: number = MAX_SLOTS): Slot[] {
  const offset = (spec.seed % 628) / 100;
  const n = Math.max(1, Math.min(count, MAX_SLOTS));
  return Array.from({ length: n }, (_, i) => {
    const a = offset + (i / n) * Math.PI * 2;
    return { x: spec.x + Math.cos(a) * spec.radius * SLOT_RING, z: spec.z + Math.sin(a) * spec.radius * SLOT_RING };
  });
}

/** Seats in the order they are given out: opposite seats first, so a few agents still spread round the ring (bit-reversed 0..7). */
export const SEAT_ORDER: readonly number[] = [0, 4, 2, 6, 1, 5, 3, 7];

/**
 * A seat on the ring for each agent that has one. An agent already seated keeps its seat; a newcomer takes the first
 * free one, so one agent arriving or leaving moves nobody. An agent with no free seat is left out of the result.
 */
export function assignSeats(prev: ReadonlyMap<string, number>, ids: readonly string[]): Map<string, number> {
  const seats = new Map<string, number>();
  const taken = new Set<number>();
  for (const id of ids) {
    const seat = prev.get(id);
    if (seat !== undefined && !taken.has(seat) && SEAT_ORDER.includes(seat)) {
      seats.set(id, seat);
      taken.add(seat);
    }
  }
  for (const id of ids) {
    if (seats.has(id)) continue;
    const free = SEAT_ORDER.find((s) => !taken.has(s));
    if (free === undefined) break;
    seats.set(id, free);
    taken.add(free);
  }
  return seats;
}

/** What a tag says of an agent: its title, cut to fit beside others. */
export function shortName(title: string, max = 14): string {
  const t = title.trim() || "Agent";
  return t.length <= max ? t : `${t.slice(0, max - 1).trimEnd()}\u2026`;
}

export interface Bounds {
  readonly minX: number;
  readonly maxX: number;
  readonly minZ: number;
  readonly maxZ: number;
}

/** The archipelago's box plus open sea around it: the camera target may not leave it. */
export function worldBounds(islands: readonly IslandSpec[], pad: number): Bounds {
  return {
    minX: Math.min(...islands.map((i) => i.x - i.radius)) - pad,
    maxX: Math.max(...islands.map((i) => i.x + i.radius)) + pad,
    minZ: Math.min(...islands.map((i) => i.z - i.radius)) - pad,
    maxZ: Math.max(...islands.map((i) => i.z + i.radius)) + pad,
  };
}

export function clampToBounds(x: number, z: number, b: Bounds): Slot {
  return { x: Math.min(Math.max(x, b.minX), b.maxX), z: Math.min(Math.max(z, b.minZ), b.maxZ) };
}

/** Smallest gap of open sea between any two islands, counting the widest their coastlines can reach. */
export function closestPair(islands: readonly IslandSpec[]): number {
  let best = Infinity;
  for (let i = 0; i < islands.length; i++) {
    for (let j = i + 1; j < islands.length; j++) {
      const a = islands[i];
      const b = islands[j];
      if (a === undefined || b === undefined) continue;
      best = Math.min(best, Math.hypot(a.x - b.x, a.z - b.z) - a.radius * 1.23 - b.radius * 1.23);
    }
  }
  return best;
}
