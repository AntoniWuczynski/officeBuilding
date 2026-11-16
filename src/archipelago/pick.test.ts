import { describe, expect, it } from "vitest";
import { boxAt } from "./pick";

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
