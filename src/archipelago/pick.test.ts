import { describe, expect, it } from "vitest";
import { boxAt, roomFor } from "./pick";

const box = (key: string, left: number, top: number, right: number, bottom: number, z = 0) => ({ key, left, top, right, bottom, z });

describe("boxAt", () => {
  it("finds the figure under the pointer, and nothing in open sea", () => {
    const boxes = [box("a", 0, 0, 30, 60), box("b", 100, 0, 130, 60)];
    expect(boxAt(boxes, 115, 30)).toBe("b");
    expect(boxAt(boxes, 60, 30)).toBeUndefined();
  });

  it("takes the nearer figure where two overlap: the one lower on screen, which the loop draws on top", () => {
    const boxes = [box("far", 0, 0, 40, 60, 60), box("near", 20, 10, 60, 90, 90)];
    expect(boxAt(boxes, 30, 40)).toBe("near");
  });

  it("ignores a figure with no size, as a hidden one has", () => {
    expect(boxAt([box("hidden", 0, 0, 0, 0)], 0, 0)).toBeUndefined();
  });
});

describe("roomFor", () => {
  const host = { left: 0, top: 0, right: 1000, bottom: 800 };
  const list = { left: 16, top: 16, right: 240, bottom: 784 };
  const signs = { left: 800, top: 16, right: 984, bottom: 784 };

  it("takes the open strip between a left list and a right column of signs, a gap clear of both", () => {
    expect(roomFor(host, [list, signs], 16, 100, 100)).toEqual({ left: 256, top: 16, right: 784, bottom: 784 });
  });

  it("uses the sea below a short list instead of giving up its whole column", () => {
    const shortList = { left: 16, top: 16, right: 240, bottom: 200 };
    const box = roomFor(host, [shortList], 16, 900, 100);
    expect(box).toEqual({ left: 16, top: 216, right: 984, bottom: 784 });
  });

  it("picks the side the content fits best, and returns null when covers fill the host", () => {
    const wide = roomFor(host, [{ left: 0, top: 0, right: 1000, bottom: 700 }], 16, 800, 40);
    expect(wide).toEqual({ left: 16, top: 716, right: 984, bottom: 784 });
    expect(roomFor(host, [host], 16, 10, 10)).toBeNull();
  });
});
