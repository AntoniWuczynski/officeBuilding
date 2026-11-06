import { useRef } from "react";
import type { ReactNode } from "react";
import { projectedBounds, worldTransform } from "../lib/iso";
import type { Camera } from "../lib/iso";
import { CameraContext } from "./Iso";
import { useElementSize } from "./useElementSize";
import { MAX_ZOOM, MIN_ZOOM, usePanZoom } from "./usePanZoom";

export interface Padding {
  readonly left: number;
  readonly right: number;
  readonly top: number;
  readonly bottom: number;
}

interface StageProps {
  readonly camera: Camera;
  /** World footprint and height in world pixels. */
  readonly width: number;
  readonly depth: number;
  readonly height: number;
  /** Screen-space room for things that hang outside the footprint (nameplates, signs). */
  readonly pad: Padding;
  /** contain: whole scene visible. width: fill the width and scroll vertically. */
  readonly fit: "contain" | "width";
  /** Cap on the fitted scale; uncapped when left out. */
  readonly maxScale?: number;
  /** Let the viewer zoom and pan freely on top of the fit. */
  readonly zoomable?: boolean;
  readonly className?: string;
  readonly children: ReactNode;
}

/** Fits a 3D world into its container and provides the camera to billboards. */
export function Stage({ camera, width, depth, height, pad, fit, maxScale = Infinity, zoomable = false, className, children }: StageProps): React.ReactElement {
  const ref = useRef<HTMLDivElement>(null);
  const size = useElementSize(ref);
  const { view, dragging, zoomBy, reset, handlers } = usePanZoom(ref, zoomable);
  const b = projectedBounds(camera, width, depth, height);
  const boxW = b.maxX - b.minX + pad.left + pad.right;
  const boxH = b.maxY - b.minY + pad.top + pad.bottom;
  const measured = size.width > 0 && size.height > 0;
  const scale = !measured
    ? 1
    : fit === "contain"
      ? Math.min(size.width / boxW, size.height / boxH, maxScale)
      : Math.min(size.width / boxW, maxScale);
  const offsetX = Math.max(0, (size.width - boxW * scale) / 2);
  const offsetY = Math.max(0, (size.height - boxH * scale) / 2);
  // World centre inside the camera box.
  const centreX = pad.left - b.minX;
  const centreY = pad.top - b.minY;
  // Scale by CSS zoom, not scale(): WebKit draws each 3D layer at its laid-out size,
  // so a scale() above the world stretches a bitmap and blurs. Zoom also multiplies
  // the camera's own translate, hence the division.
  const zoom = view.zoom * scale;
  const shiftX = (view.x + view.zoom * offsetX) / zoom;
  const shiftY = (view.y + view.zoom * offsetY) / zoom;

  return (
    <div
      ref={ref}
      className={className === undefined ? "stage" : `stage ${className}`}
      data-fit={fit}
      data-zoomable={zoomable ? "" : undefined}
      data-dragging={dragging ? "" : undefined}
      {...handlers}
    >
      <div className="stage-sizer" style={{ height: Math.max(size.height, boxH * scale + offsetY * 2) }}>
        <div
          className="stage-camera"
          style={{ width: boxW, height: boxH, zoom, transform: `translate(${shiftX}px, ${shiftY}px)` }}
        >
          <div
            className="world"
            style={{ left: centreX - width / 2, top: centreY - depth / 2, width, height: depth, transform: worldTransform(camera) }}
          >
            <CameraContext.Provider value={camera}>{children}</CameraContext.Provider>
          </div>
        </div>
      </div>
      {zoomable ? (
        <div className="zoom-controls" role="group" aria-label="Zoom">
          <button type="button" className="btn" aria-label="Zoom out" disabled={view.zoom <= MIN_ZOOM} onClick={() => zoomBy(1 / 1.25)}>
            −
          </button>
          <button type="button" className="btn" data-testid="zoom-fit" onClick={reset}>
            Fit
          </button>
          <button type="button" className="btn" aria-label="Zoom in" disabled={view.zoom >= MAX_ZOOM} onClick={() => zoomBy(1.25)}>
            +
          </button>
        </div>
      ) : null}
    </div>
  );
}
