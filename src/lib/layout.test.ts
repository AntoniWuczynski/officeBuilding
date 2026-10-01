import { describe, expect, it } from "vitest";
import {
  DESK,
  ROOM,
  STOREY,
  doorOf,
  floorSpotOf,
  roomDepth,
  roomDeskSlot,
  seatOf,
  storeyDeskCount,
  storeyDeskX,
  storeyWidth,
} from "./layout";

describe("room layout", () => {
  it("fills three columns left to right, then the next row", () => {
    expect(roomDeskSlot(0)).toEqual({ x: ROOM.colX0, y: ROOM.rowY0 });
    expect(roomDeskSlot(2)).toEqual({ x: ROOM.colX0 + 2 * ROOM.colGap, y: ROOM.rowY0 });
    expect(roomDeskSlot(3)).toEqual({ x: ROOM.colX0, y: ROOM.rowY0 + ROOM.rowGap });
  });
  it("keeps every desk clear of the manager's office", () => {
    for (let i = 0; i < 9; i++) {
      const d = roomDeskSlot(i);
      const overlapsOffice = d.x + DESK.w > ROOM.managerX && d.y < ROOM.managerDepth;
      expect(overlapsOffice).toBe(false);
    }
  });
  it("grows deeper as rows are added, never below the minimum", () => {
    expect(roomDepth(0)).toBe(ROOM.minDepth);
    expect(roomDepth(4)).toBe(ROOM.minDepth);
    expect(roomDepth(10)).toBeGreaterThan(roomDepth(4));
    const last = roomDeskSlot(9);
    expect(roomDepth(10)).toBeGreaterThanOrEqual(last.y + DESK.d);
  });
  it("seats the worker behind the desk and lays a stuck one in front", () => {
    const desk = { x: 100, y: 60 };
    expect(seatOf(desk).y).toBeLessThan(desk.y);
    expect(floorSpotOf(desk, 500).y).toBeGreaterThan(desk.y + DESK.d);
    expect(floorSpotOf(desk, 70).y).toBe(70);
  });
  it("walks new hires in from the open front", () => {
    expect(doorOf(300)).toEqual({ x: ROOM.width - 24, y: 284 });
  });
});

describe("storey layout", () => {
  it("shows at most the storey's desk budget", () => {
    expect(storeyDeskCount(2)).toBe(2);
    expect(storeyDeskCount(20)).toBe(STOREY.maxDesks);
  });
  it("sizes the building for the busiest floor within limits", () => {
    expect(storeyWidth(0)).toBe(storeyWidth(STOREY.minDesks));
    expect(storeyWidth(4)).toBeGreaterThan(storeyWidth(3));
    expect(storeyWidth(50)).toBe(storeyWidth(STOREY.maxDesks));
  });
  it("spaces desks evenly", () => {
    expect(storeyDeskX(1) - storeyDeskX(0)).toBe(STOREY.deskGap);
  });
});
