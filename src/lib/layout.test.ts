import { describe, expect, it } from "vitest";
import {
  DESK,
  MANAGER_DESK,
  ROOM,
  managerSeat,
  STOREY,
  doorOf,
  POD,
  floorSpotOf,
  podFor,
  QA_DESK,
  qaDeskSlot,
  roomDepth,
  roomDeskSlot,
  ZONE,
  partitionY,
  zoneBounds,
  zoneDeskSlot,
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

describe("QA corner desk", () => {
  it("stands in the front-left corner", () => {
    expect(qaDeskSlot(300)).toEqual({ x: QA_DESK.x, y: 300 - QA_DESK.back });
  });

  it("adds no depth to a room at its minimum", () => {
    expect(roomDepth(2, true)).toBe(roomDepth(2));
  });

  it("makes a room with three rows deeper by the corner's extra", () => {
    expect(roomDepth(7, true)).toBe(roomDepth(7) + QA_DESK.extra);
  });
});

describe("the manager's desk", () => {
  it("stands inside the glass office, with the manager behind it", () => {
    expect(MANAGER_DESK.x).toBeGreaterThanOrEqual(ROOM.managerX);
    expect(MANAGER_DESK.x + DESK.w).toBeLessThanOrEqual(ROOM.width);
    expect(MANAGER_DESK.y + DESK.d).toBeLessThanOrEqual(ROOM.managerDepth);
    const seat = managerSeat();
    expect(seat.x).toBeGreaterThan(MANAGER_DESK.x);
    expect(seat.x).toBeLessThan(MANAGER_DESK.x + DESK.w);
    expect(seat.y).toBeLessThanOrEqual(MANAGER_DESK.y);
  });
});

interface Box {
  readonly name: string;
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly d: number;
}

function overlap(a: Box, b: Box): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.d && b.y < a.y + a.d;
}

const deskBox = (name: string, at: { readonly x: number; readonly y: number }): Box => ({ name, x: at.x, y: at.y, w: DESK.w, d: DESK.d });

describe("the manager's team zone", () => {
  it("is absent, and the grid unchanged, when the manager hired nothing", () => {
    expect(partitionY(0)).toBeNull();
    expect(zoneBounds(0)).toBeUndefined();
    for (let i = 0; i < 12; i++) expect(roomDeskSlot(i, 0)).toEqual(roomDeskSlot(i));
    expect(roomDepth(7, true, 0)).toBe(roomDepth(7, true));
  });

  for (let team = 1; team <= 12; team++) {
    it(`keeps ${team} zone desk(s) clear of the office, the partition, the human desks, the QA desk and each other, for 0 to 9 human desks`, () => {
      const line = partitionY(team);
      if (line === null) throw new Error("no partition");
      const office: Box = { name: "office", x: ROOM.managerX, y: 0, w: ROOM.width - ROOM.managerX, d: ROOM.managerDepth };
      const partition: Box = { name: "partition", x: 0, y: line - 1, w: ROOM.width, d: 2 };
      const zone = Array.from({ length: team }, (_, i) => deskBox(`zone ${i}`, zoneDeskSlot(i)));
      for (let human = 0; human <= 9; human++) {
        const depth = roomDepth(human + 1, true, team);
        const mine = Array.from({ length: human + 1 }, (_, i) => deskBox(i === human ? "hire slot" : `human ${i}`, roomDeskSlot(i, team)));
        const qa = deskBox("qa", qaDeskSlot(depth));
        const all: Box[] = [office, partition, qa, ...zone, ...mine];
        for (let a = 0; a < all.length; a++) {
          for (let b = a + 1; b < all.length; b++) {
            const [p, q] = [all[a], all[b]];
            if (p === undefined || q === undefined) throw new Error("index");
            expect(overlap(p, q), `${p.name} overlaps ${q.name} with ${team} team and ${human} human desks`).toBe(false);
          }
        }
        // The partition is between the zone and the human grid, the hire slot is in the grid, and everything is inside the room.
        for (const z of zone) expect(z.y + z.d).toBeLessThan(line);
        for (const m of mine) expect(m.y).toBeGreaterThan(line);
        for (const b of [...zone, ...mine, qa]) {
          expect(b.x + b.w).toBeLessThanOrEqual(ROOM.width);
          expect(b.y + b.d).toBeLessThanOrEqual(depth);
        }
        for (const z of zone) expect(z.x + z.w).toBeLessThanOrEqual(ROOM.managerX);
      }
    });
  }

  it("keeps the manager's front glass clear of the partition", () => {
    expect(partitionY(1)).toBeGreaterThanOrEqual(ROOM.managerDepth + ZONE.officeGap);
  });

  it("keeps a zone desk's helpers inside the zone, left of the office and short of the partition", () => {
    for (let team = 1; team <= 12; team++) {
      const line = partitionY(team);
      const bounds = zoneBounds(team);
      if (line === null || bounds === undefined) throw new Error("no zone");
      for (let i = 0; i < team; i++) {
        const desk = zoneDeskSlot(i);
        const pod = podFor(desk, 9, bounds);
        for (const slot of pod.slots) {
          // Left of the office only where the pod reaches down beside it; below it the pod keeps the grid's width.
          const reach = pod.region.y < ROOM.managerDepth ? ROOM.managerX - 6 : desk.x + ROOM.colGap - 6;
          expect(slot.x + pod.cell).toBeLessThanOrEqual(reach + 1e-9);
          expect(slot.y + pod.cell).toBeLessThanOrEqual(line - ZONE.podGap + 1e-9);
        }
      }
    }
  });

  it("gives a zone desk below the office's depth the same pod width as in the grid, and caps one beside the office", () => {
    const bounds = zoneBounds(9);
    if (bounds === undefined) throw new Error("no zone");
    for (let i = 0; i < 9; i++) {
      const desk = zoneDeskSlot(i);
      const pod = podFor(desk, 1, bounds);
      if (desk.y - 22 >= ROOM.managerDepth) expect(pod.region.w).toBe(podFor(desk, 1).region.w);
      else expect(pod.region.x + pod.region.w).toBeLessThanOrEqual(ROOM.managerX - 6);
    }
    const below = podFor(zoneDeskSlot(6), 1, bounds).region;
    expect(below.w).toBeGreaterThan(40);
  });

  it("grows the room deeper than the same human desks without a zone", () => {
    expect(roomDepth(10, false, 3)).toBeGreaterThan(roomDepth(10, false, 0));
  });
});
