// Where things stand in the two scenes. Pure numbers in world pixels, so the
// layout is testable and the Rust backend never has to know about desks.

export const DESK = { w: 36, d: 18, h: 12 } as const;

export interface Slot {
  readonly x: number;
  readonly y: number;
}

/** Inside one floor (FLOOR_CAMERA). */
export const ROOM = {
  width: 420,
  slab: 10,
  wallHeight: 92,
  columns: 3,
  colX0: 28,
  colGap: 98,
  rowY0: 58,
  rowGap: 78,
  frontMargin: 64,
  minDepth: 220,
  /** Manager's glass office, back-right corner. */
  managerX: 290,
  managerDepth: 112,
  glassHeight: 64,
} as const;

/** The manager's team zone: the desks it hired, fenced off at the back, left of its office. */
export const ZONE = {
  /** Desks across: the three columns that fit left of the manager's office. */
  columns: ROOM.columns,
  /** From the front edge of the zone's last desk to the partition. */
  partitionGap: 42,
  /** From the partition to the back edge of the first row of the human desks. */
  humanGap: 36,
  /** Least clearance between the manager's front glass and the partition. */
  officeGap: 8,
  /** How far the partition stands from a zone desk's helpers. */
  podGap: 6,
  partitionHeight: 22,
} as const;

/** Top-left corner of zone desk number `index`: the same columns and rows as the grid, in the back strip. */
export function zoneDeskSlot(index: number): Slot {
  return { x: ROOM.colX0 + (index % ZONE.columns) * ROOM.colGap, y: ROOM.rowY0 + Math.floor(index / ZONE.columns) * ROOM.rowGap };
}

/** The line of the low glass partition across the room, or null when the manager hired nothing. */
export function partitionY(teamDesks: number): number | null {
  if (teamDesks <= 0) return null;
  const lastRowY = zoneDeskSlot(teamDesks - 1).y;
  return Math.max(lastRowY + DESK.d + ZONE.partitionGap, ROOM.managerDepth + ZONE.officeGap);
}

/** Where the first row of the human desks stands: the back strip, or just beyond the partition. */
function humanRowY0(teamDesks: number): number {
  const line = partitionY(teamDesks);
  return line === null ? ROOM.rowY0 : line + ZONE.humanGap;
}

/**
 * Top-left corner of desk number `index` (sessions first, then the hire desk). With `teamDesks` desks in the manager's
 * zone, the grid starts beyond the partition.
 */
export function roomDeskSlot(index: number, teamDesks = 0): Slot {
  const col = index % ROOM.columns;
  const row = Math.floor(index / ROOM.columns);
  return { x: ROOM.colX0 + col * ROOM.colGap, y: humanRowY0(teamDesks) + row * ROOM.rowGap };
}

/** The manager's desk: its top-left corner, in the glass office. */
export const MANAGER_DESK: Slot = { x: ROOM.managerX + 46, y: 40 };

/** Where the manager stands behind its desk, and where a note it carries starts. */
export function managerSeat(): Slot {
  return { x: MANAGER_DESK.x + 25, y: 38 };
}

/** The QA desk: the floor's front-left corner, where the plant room keeps its tank. */
export const QA_DESK = { x: 10, back: 44, extra: 26 } as const;

/** Top-left corner of the QA desk in a room of `depth`. */
export function qaDeskSlot(depth: number): Slot {
  return { x: QA_DESK.x, y: depth - QA_DESK.back };
}

/** Room depth that fits `deskCount` desks (and a team zone of `teamDesks`) plus the front walkway, and the corner desk when `corner` is set. */
export function roomDepth(deskCount: number, corner = false, teamDesks = 0): number {
  const rows = Math.max(1, Math.ceil(deskCount / ROOM.columns));
  const lastRowY = humanRowY0(teamDesks) + (rows - 1) * ROOM.rowGap;
  return Math.max(ROOM.minDepth, lastRowY + DESK.d + ROOM.frontMargin + (corner ? QA_DESK.extra : 0));
}

/** The tiny desks a session's helpers (sub-agents) get beside its own. */
export const POD = {
  /** Past this many, the rest show as a count. */
  cap: 64,
  /** Grid cell of a lone helper's desk. */
  maxCell: 26,
  /** Smallest cell that still seats a person. */
  personCell: 15,
  /** A helper's desk, as a share of its cell's side. */
  deskShare: 0.78,
} as const;

export interface Region {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

export interface Pod {
  readonly region: Region;
  /** Side of one grid cell; a helper's desk is drawn inside it. */
  readonly cell: number;
  /** Top-left corner of each drawn helper desk. */
  readonly slots: readonly Slot[];
  /** Helpers past the cap, shown as a count. */
  readonly hidden: number;
}

/** Edges a pod must stay inside, for a desk in the manager's zone. */
export interface PodBounds {
  readonly right: number;
  readonly bottom: number;
}

/** The pod bounds of a zone desk when the zone holds `teamDesks`: short of the manager's office and the partition. */
export function zoneBounds(teamDesks: number): PodBounds | undefined {
  const line = partitionY(teamDesks);
  return line === null ? undefined : { right: ROOM.managerX - 6, bottom: line - ZONE.podGap };
}

/** The floor patch beside a desk where its helpers sit, clipped short of the manager's office. */
function podRegion(desk: Slot, bounds: PodBounds | undefined): Region {
  const x = desk.x + DESK.w + 8;
  const y = desk.y - 22;
  const natural = y < ROOM.managerDepth ? Math.min(desk.x + ROOM.colGap - 6, ROOM.managerX - 6) : desk.x + ROOM.colGap - 6;
  // The office's edge only matters where the pod reaches down beside the office.
  const right = bounds === undefined || y >= ROOM.managerDepth ? natural : Math.min(natural, bounds.right);
  const h = bounds === undefined ? 92 : Math.min(92, bounds.bottom - y);
  return { x, y, w: right - x, h };
}

/** Lay out `count` helpers beside `desk` on the most square grid that fits (inside `bounds` when given). */
export function podFor(desk: Slot, count: number, bounds?: PodBounds): Pod {
  const region = podRegion(desk, bounds);
  const shown = Math.min(count, POD.cap);
  let cols = 1;
  let cell = 0;
  for (let c = 1; c <= shown; c++) {
    const fit = Math.min(region.w / c, region.h / Math.ceil(shown / c), POD.maxCell);
    if (fit > cell) {
      cols = c;
      cell = fit;
    }
  }
  const slots = Array.from({ length: shown }, (_, i) => ({
    x: region.x + (i % cols) * cell,
    y: region.y + Math.floor(i / cols) * cell,
  }));
  return { region, cell, slots, hidden: count - shown };
}

/** Where someone stands at a desk, and how large they are drawn (1 at a full desk). */
export interface Seat {
  readonly at: Slot;
  readonly scale: number;
}

/** Where a helper stands at its tiny desk in `slot`, or null once the pod is too small to seat people. */
export function podSeatOf(pod: Pod, slot: Slot): Seat | null {
  if (pod.cell < POD.personCell) return null;
  return { at: { x: slot.x + pod.cell * POD.deskShare * 0.62, y: slot.y - 1 }, scale: pod.cell / 40 };
}

/** Where a seated worker stands: just behind the desk, facing the camera. */
export function seatOf(desk: Slot): Slot {
  return { x: desk.x + 25, y: desk.y - 2 };
}

/** Where a stuck agent lies: on the floor in front of their desk, never hidden by it. */
export function floorSpotOf(desk: Slot, maxY: number): Slot {
  return { x: desk.x + DESK.w / 2, y: Math.min(desk.y + DESK.d + 18, maxY) };
}

/** Walk-in point: the open front-right corner of the room. */
export function doorOf(depth: number): Slot {
  return { x: ROOM.width - 24, y: depth - 16 };
}

/** One storey of the building exterior (BUILDING_CAMERA). */
export const STOREY = {
  depth: 80,
  height: 56,
  slab: 9,
  deskY: 50,
  deskX0: 22,
  deskGap: 62,
  managerWidth: 74,
  minDesks: 3,
  maxDesks: 6,
} as const;

/** How many desks a storey shows for a floor with `sessionCount` sessions. */
export function storeyDeskCount(sessionCount: number): number {
  return Math.min(sessionCount, STOREY.maxDesks);
}

/** Building width that fits the busiest floor. */
export function storeyWidth(maxSessions: number): number {
  const desks = Math.min(Math.max(maxSessions, STOREY.minDesks), STOREY.maxDesks);
  return STOREY.deskX0 + desks * STOREY.deskGap + STOREY.managerWidth + 12;
}

export function storeyDeskX(index: number): number {
  return STOREY.deskX0 + index * STOREY.deskGap;
}
