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
  /** The folder the repo lives in: repos side by side form one group of islands. */
  readonly group: string;
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
/** Radii run 22..26 and the coastline swings up to 23% either way (build.ts blob): the widest island reaches 32. */
const MAX_REACH = 32;
/**
 * Spiral constant: on r = SPACING * sqrt(i) no two islands sit closer than SPACING (measured for 2 to 40 islands in a
 * group), so 92 leaves at least 92 - 2 * MAX_REACH = 28 units of open sea.
 */
const SPACING = 92;
/** Open sea between groups. */
const GROUP_GAP = 70;
/** Room a group reserves: for the next power of two of islands, so one more repo only moves its neighbours when the folder doubles. */
function groupRoom(members: number): number {
  return 2 ** Math.ceil(Math.log2(Math.max(1, members)));
}

/** How far a group's spiral can reach from its centre, for the room reserved for it. */
function groupReach(members: number): number {
  return SPACING * Math.sqrt(groupRoom(members)) + MAX_REACH;
}

function groupOf(project: Project): string {
  const i = project.path.lastIndexOf("/");
  return i <= 0 ? "/" : project.path.slice(0, i);
}

/** Projects in a stable order: ones already placed keep their place, new ones go last. Removed ones are never dropped. */
export function mergeOrder(order: readonly string[], currentIds: readonly string[]): string[] {
  const known = new Set(order);
  return [...order, ...currentIds.filter((id) => !known.has(id))];
}

/**
 * Each group of repos is a golden-angle spiral of islands; groups sit side by side along x, in the order they first
 * appear, with open sea between. A new repo takes the next place on its group's spiral and a new folder starts a new
 * group at the end of the row, so neither moves an island already placed. A group that outgrows its reserved room
 * (groupRoom: when the folder passes a power of two) moves the groups after it, and itself unless it is the first.
 * The setup project becomes the lighthouse islet.
 */
export function layoutIslands(projects: readonly Project[], setupProjectId: string): IslandSpec[] {
  const groups = new Map<string, number>();
  for (const p of projects) groups.set(groupOf(p), (groups.get(groupOf(p)) ?? 0) + 1);
  // The first group is centred on the origin; each next centre sits one reach past the previous group's.
  const centreOf = new Map<string, number>();
  let previous: { readonly centre: number; readonly reach: number } | null = null;
  for (const [key, members] of groups) {
    const reach = groupReach(members);
    const centre: number = previous === null ? 0 : previous.centre + previous.reach + GROUP_GAP + reach;
    centreOf.set(key, centre);
    previous = { centre, reach };
  }

  const index = new Map<string, number>();
  return projects.map((project) => {
    const group = groupOf(project);
    const i = index.get(group) ?? 0;
    index.set(group, i + 1);
    const a = i * GOLDEN_ANGLE;
    const r = i === 0 ? 0 : SPACING * Math.sqrt(i);
    const isSetup = project.id === setupProjectId;
    return {
      projectId: project.id,
      name: project.name,
      x: (centreOf.get(group) ?? 0) + Math.cos(a) * r,
      z: Math.sin(a) * r,
      radius: isSetup ? 11 : 22 + (hash(project.id) % 5),
      theme: isSetup ? "lighthouse" : themeFor(project),
      group,
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

/** World positions of an island's seats on a ring, offset by its seed so islands do not look alike. */
export function agentSlots(spec: IslandSpec): Slot[] {
  const offset = (spec.seed % 628) / 100;
  return Array.from({ length: MAX_SLOTS }, (_, i) => {
    const a = offset + (i / MAX_SLOTS) * Math.PI * 2;
    return { x: spec.x + Math.cos(a) * spec.radius * SLOT_RING, z: spec.z + Math.sin(a) * spec.radius * SLOT_RING };
  });
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
