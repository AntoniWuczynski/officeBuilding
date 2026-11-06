import { describe, expect, it } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import { FLOOR_CAMERA } from "../lib/iso";
import { Stage } from "./Stage";
import { MAX_ZOOM, MIN_ZOOM, zoomAround } from "./usePanZoom";

describe("zoomAround", () => {
  it("keeps the point under the pointer still", () => {
    const before = { zoom: 1.5, x: 20, y: -10 };
    const after = zoomAround(before, 2, 300, 200);
    // Stage point (300, 200) maps to the same content point before and after.
    expect((300 - after.x) / after.zoom).toBeCloseTo((300 - before.x) / before.zoom);
    expect((200 - after.y) / after.zoom).toBeCloseTo((200 - before.y) / before.zoom);
  });

  it("clamps the zoom and leaves the view still at the limit", () => {
    expect(zoomAround({ zoom: 1, x: 0, y: 0 }, 100, 50, 50).zoom).toBe(MAX_ZOOM);
    expect(zoomAround({ zoom: MIN_ZOOM, x: 5, y: 5 }, 0.5, 50, 50)).toEqual({ zoom: MIN_ZOOM, x: 5, y: 5 });
  });
});

function renderStage(): { stage: HTMLElement; zoom: () => number } {
  const { container } = render(
    <Stage camera={FLOOR_CAMERA} width={400} depth={300} height={100} pad={{ left: 0, right: 0, top: 0, bottom: 0 }} fit="contain" zoomable>
      <div />
    </Stage>,
  );
  const stage = container.querySelector<HTMLElement>(".stage");
  const camera = container.querySelector<HTMLElement>(".stage-camera");
  if (stage === null || camera === null) throw new Error("Stage did not render");
  // The scale is CSS zoom, not a transform: WebKit draws 3D layers at their laid-out
  // size, so a scale() above the world would stretch a bitmap and blur.
  return { stage, zoom: () => Number(camera.style.zoom) };
}

describe("scroll to zoom, drag to pan", () => {
  it("scrolling up zooms in and scrolling down zooms out", () => {
    const { stage, zoom } = renderStage();
    fireEvent.wheel(stage, { deltaY: -40 });
    expect(zoom()).toBeGreaterThan(1);
    fireEvent.wheel(stage, { deltaY: 40 });
    fireEvent.wheel(stage, { deltaY: 40 });
    expect(zoom()).toBeLessThan(1);
  });

  it("dragging pans without zooming", () => {
    const { stage, zoom } = renderStage();
    const camera = stage.querySelector<HTMLElement>(".stage-camera");
    fireEvent.pointerDown(stage, { pointerId: 1, button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(stage, { pointerId: 1, clientX: 120, clientY: 110 });
    expect(camera?.style.transform).toBe("translate(20px, 10px)");
    expect(zoom()).toBe(1);
  });
});
