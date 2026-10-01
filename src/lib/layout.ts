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

/** Top-left corner of desk number `index` (sessions first, then the hire desk). */
export function roomDeskSlot(index: number): Slot {
  const col = index % ROOM.columns;
  const row = Math.floor(index / ROOM.columns);
  return { x: ROOM.colX0 + col * ROOM.colGap, y: ROOM.rowY0 + row * ROOM.rowGap };
}

/** Room depth that fits `deskCount` desks plus the front walkway. */
export function roomDepth(deskCount: number): number {
  const rows = Math.max(1, Math.ceil(deskCount / ROOM.columns));
  const lastRowY = ROOM.rowY0 + (rows - 1) * ROOM.rowGap;
  return Math.max(ROOM.minDepth, lastRowY + DESK.d + ROOM.frontMargin);
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
