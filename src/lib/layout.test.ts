import { describe, expect, it } from "vitest";
import {
  DESK,
  ROOM,
  STOREY,
  doorOf,
  POD,
  floorSpotOf,
  podFor,
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

describe("helper pods", () => {
  it("sit beside their desk, clear of the next desk and the manager's office", () => {
    for (let i = 0; i < 9; i++) {
      const desk = roomDeskSlot(i);
      const { region } = podFor(desk, 1);
      expect(region.x).toBeGreaterThanOrEqual(desk.x + DESK.w);
      expect(region.x + region.w).toBeLessThanOrEqual(desk.x + ROOM.colGap);
      expect(region.w).toBeGreaterThan(0);
      expect(region.x + region.w > ROOM.managerX && region.y < ROOM.managerDepth).toBe(false);
    }
  });

  it("shrink the desks as the team grows, up to the cap, then fold the rest into a count", () => {
    const desk = roomDeskSlot(0);
    const one = podFor(desk, 1);
    const full = podFor(desk, POD.cap);
    const huge = podFor(desk, 250);
    expect(one.slots).toHaveLength(1);
    expect(one.cell).toBe(POD.maxCell);
    expect(full.cell).toBeLessThan(one.cell);
    expect(full.hidden).toBe(0);
    expect(huge.slots).toHaveLength(POD.cap);
    expect(huge.hidden).toBe(250 - POD.cap);
    expect(huge.cell).toBe(full.cell);
    for (const pod of [one, full, huge, podFor(roomDeskSlot(2), 40)]) {
      for (const s of pod.slots) {
        expect(s.x).toBeGreaterThanOrEqual(pod.region.x);
        expect(s.y).toBeGreaterThanOrEqual(pod.region.y);
        expect(s.x + pod.cell).toBeLessThanOrEqual(pod.region.x + pod.region.w + 1e-9);
        expect(s.y + pod.cell).toBeLessThanOrEqual(pod.region.y + pod.region.h + 1e-9);
      }
    }
  });
});
