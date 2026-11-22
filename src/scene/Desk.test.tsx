import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { CameraContext } from "./Iso";
import { Desk } from "./Desk";
import { FLOOR_CAMERA } from "../lib/iso";

const desk = (props: Partial<React.ComponentProps<typeof Desk>>): HTMLElement => {
  const { container } = render(
    <CameraContext value={FLOOR_CAMERA}>
      <Desk x={0} y={0} z={0} {...props} />
    </CameraContext>,
  );
  return container as HTMLElement;
};
const woodClasses = (el: HTMLElement): string[] => [...el.querySelectorAll("[class*='wood-']")].flatMap((e) => [...e.classList].filter((c) => c.startsWith("wood-")));

describe("a desk's wood", () => {
  it("carries the wood class in the office", () => {
    expect(woodClasses(desk({ wood: "wenge" }))).toEqual(["wood-wenge"]);
  });

  it("carries the wood class on an island", () => {
    expect(woodClasses(desk({ wood: "maple", look: "island" }))).toEqual(["wood-maple"]);
  });

  it("has no wood class without one", () => {
    expect(woodClasses(desk({}))).toEqual([]);
    expect(woodClasses(desk({ look: "island" }))).toEqual([]);
  });
});
