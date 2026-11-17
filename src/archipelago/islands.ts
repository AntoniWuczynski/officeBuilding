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
    return specFor(project, Math.cos(a) * r, Math.sin(a) * r, setupProjectId);
  });
}

function specFor(project: Project, x: number, z: number, setupProjectId: string): IslandSpec {
  const isSetup = project.id === setupProjectId;
  return {
    projectId: project.id,
    name: project.name,
    x,
    z,
    radius: isSetup ? 11 : 22 + (hash(project.id) % 5),
    theme: isSetup ? "lighthouse" : themeFor(project),
    seed: hash(project.id),
  };
}

/**
 * Fixed slot `i` of a layout whose neighbours sit `step` apart: slot 0 in the middle, then rings of as many slots as
 * fit at that spacing (7, 15, 23 ...), each ring 1.3 steps beyond the last. The slot never depends on what else is there.
 */
export function ringSlot(i: number, step: number): { x: number; z: number } {
  if (i === 0) return { x: 0, z: 0 };
  let left = i - 1;
  for (let ring = 1; ; ring++) {
    const r = step * (1.2 + 1.3 * (ring - 1));
    const n = Math.floor(Math.PI / Math.asin(step / (2 * r)));
    if (left < n) {
      const a = (left / n) * Math.PI * 2;
      return { x: Math.cos(a) * r, z: Math.sin(a) * r };
    }
    left -= n;
  }
}

/** Centre-to-centre distance of clusters: a full ring of 8 islands reaches 110 + 32 from its centre, so 92 more sea between two of them. */
export const CLUSTER_STEP = 330;

export interface SlotAt {
  readonly cluster: string;
  readonly slot: number;
}

/**
 * A slot in its cluster for each repo. A repo keeps its slot while it stays in its cluster; a newcomer, or a repo that
 * changed cluster, takes the first free slot of the cluster it is in now. Repos no longer listed keep theirs (a hole of sea).
 */
export function assignSlots(prev: ReadonlyMap<string, SlotAt>, groups: ReadonlyMap<string, readonly string[]>): Map<string, SlotAt> {
  const next = new Map<string, SlotAt>();
  const taken = new Map<string, Set<number>>();
  const take = (id: string, at: SlotAt): void => {
    next.set(id, at);
    taken.set(at.cluster, (taken.get(at.cluster) ?? new Set<number>()).add(at.slot));
  };
  const listed = new Map<string, string>();
  for (const [cluster, ids] of groups) for (const id of ids) listed.set(id, cluster);
  for (const [id, at] of prev) if (!listed.has(id) || listed.get(id) === at.cluster) take(id, at);
  for (const [cluster, ids] of groups) {
    for (const id of ids) {
      if (next.has(id)) continue;
      let slot = 0;
      while (taken.get(cluster)?.has(slot) === true) slot++;
      take(id, { cluster, slot });
    }
  }
  return next;
}

/**
 * Islands grouped in clusters: each cluster sits on a fixed ring slot by its first-seen index (`clusterOrder`), each
 * repo on its slot inside it, the setup project on the lighthouse slot. Repos with no slot, and clusters with no repo, are not drawn.
 */
export function layoutClusters(
  clusterOrder: readonly string[],
  slots: ReadonlyMap<string, SlotAt>,
  projects: readonly Project[],
  setupProjectId: string,
): { islands: IslandSpec[]; centres: Map<string, { x: number; z: number }> } {
  const centres = new Map<string, { x: number; z: number }>();
  const islands = projects.flatMap((project) => {
    if (project.id === setupProjectId) return [];
    const at = slots.get(project.id);
    const index = at === undefined ? -1 : clusterOrder.indexOf(at.cluster);
    if (at === undefined || index < 0) return [];
    const c = ringSlot(index, CLUSTER_STEP);
    centres.set(at.cluster, c);
    const o = ringSlot(at.slot, SPACING);
    return [specFor(project, c.x + o.x, c.z + o.z, setupProjectId)];
  });
  // Placed last: it needs the occupied rings, and it moves only when a new ring opens, never a repo island for it.
  const outer = Math.max(0, ...[...centres.values()].map((c) => Math.hypot(c.x, c.z)));
  const setup = projects.find((project) => project.id === setupProjectId);
  if (setup !== undefined) islands.push(specFor(setup, 0, outer + CLUSTER_STEP, setupProjectId));
  return { islands, centres };
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
