import { describe, expect, it } from "vitest";
import {
  BUILDING_CAMERA,
  FLOOR_CAMERA,
  billboardTransform,
  headsLeft,
  project,
  projectedBounds,
  worldTransform,
} from "./iso";

describe("project", () => {
  it("puts the world centre at the origin", () => {
    expect(project(FLOOR_CAMERA, 100, 60, 50, 30, 0)).toEqual({ x: 0, y: 0 });
  });
  it("raises points with height", () => {
    const low = project(FLOOR_CAMERA, 100, 60, 50, 30, 0);
    const high = project(FLOOR_CAMERA, 100, 60, 50, 30, 40);
    expect(high.y).toBeLessThan(low.y);
    expect(high.x).toBeCloseTo(low.x, 10);
  });
  it("at 45° sends +x down-right and +y down-left", () => {
    const right = project(FLOOR_CAMERA, 0, 0, 10, 0, 0);
    const left = project(FLOOR_CAMERA, 0, 0, 0, 10, 0);
    expect(right.x).toBeGreaterThan(0);
    expect(right.y).toBeGreaterThan(0);
    expect(left.x).toBeLessThan(0);
    expect(left.y).toBeGreaterThan(0);
  });
});

describe("projectedBounds", () => {
  it("is symmetric about the centre for a flat square at 45°", () => {
    const b = projectedBounds(FLOOR_CAMERA, 100, 100, 0);
    expect(b.minX).toBeCloseTo(-b.maxX, 10);
    expect(b.minY).toBeCloseTo(-b.maxY, 10);
  });
  it("grows upwards with height", () => {
    expect(projectedBounds(BUILDING_CAMERA, 300, 80, 200).minY).toBeLessThan(projectedBounds(BUILDING_CAMERA, 300, 80, 0).minY);
  });
});

describe("transforms", () => {
  it("billboards undo the world rotation", () => {
    expect(worldTransform(BUILDING_CAMERA)).toBe("rotateX(68deg) rotateZ(18deg)");
    expect(billboardTransform(BUILDING_CAMERA, 12)).toBe("translateZ(12px) rotateZ(-18deg) rotateX(-68deg)");
  });
});

describe("headsLeft", () => {
  it("tells which way a walker faces on screen", () => {
    expect(headsLeft(FLOOR_CAMERA, 100, 100, { x: 50, y: 10 }, { x: 10, y: 50 })).toBe(true);
    expect(headsLeft(FLOOR_CAMERA, 100, 100, { x: 10, y: 50 }, { x: 50, y: 10 })).toBe(false);
  });
});
