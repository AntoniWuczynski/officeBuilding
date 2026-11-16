import { createContext, useContext } from "react";
import type { CSSProperties, ReactNode } from "react";
import { FLOOR_CAMERA, billboardTransform, project } from "../lib/iso";
import type { Camera } from "../lib/iso";

/** Inline style that may also carry CSS custom properties. */
export type StyleWithVars = CSSProperties & Record<`--${string}`, string>;

/** The camera of the enclosing Stage; billboards need it to face the viewer. */
export const CameraContext = createContext<Camera>(FLOOR_CAMERA);

function cx(base: string, extra: string | undefined): string {
  return extra === undefined ? base : `${base} ${extra}`;
}

interface BoxProps {
  readonly x: number;
  readonly y: number;
  readonly z?: number;
  readonly w: number;
  readonly d: number;
  readonly h: number;
  readonly top: string;
  readonly front: string;
  readonly side: string;
  readonly className?: string;
  readonly topContent?: ReactNode;
  readonly frontContent?: ReactNode;
}

/** A solid box standing on the floor plane; only the three visible faces are drawn. */
export function IsoBox({ x, y, z = 0, w, d, h, top, front, side, className, topContent, frontContent }: BoxProps): React.ReactElement {
  const style: StyleWithVars = {
    left: x,
    top: y,
    width: w,
    height: d,
    transform: `translateZ(${z}px)`,
    "--h": `${h}px`,
  };
  return (
    <div className={cx("iso-box", className)} style={style}>
      <i className="iso-top" style={{ background: top }}>{topContent}</i>
      <i className="iso-front" style={{ background: front }}>{frontContent}</i>
      <i className="iso-side" style={{ background: side }} />
    </div>
  );
}

interface Point3 {
  readonly x: number;
  readonly y: number;
  readonly z: number;
}

/**
 * How far a tag reaches above its anchor on screen, in px, as laid out in the app (CSS bottom plus height): a worker's
 * name tag (scene.css, 38 + 14.6), the "managed" mark over it (manager.css, 56 + 13.9), the tallest tag a seated worker
 * carries, and the hire sign (23.7).
 */
export const TAG_TOP = { name: 52.6, managed: 69.9, hire: 23.7 } as const;

/** How high a point (or a billboard's foot) over floor point `at` must be to show `gap` px above `mark` on screen. */
export function heightToClear(cam: Camera, mark: Point3, at: { readonly x: number; readonly y: number }, gap: number): number {
  const foot = project(cam, 0, 0, at.x, at.y, 0).y;
  const target = project(cam, 0, 0, mark.x, mark.y, mark.z).y - gap;
  return (foot - target) / Math.sin((cam.tilt * Math.PI) / 180);
}

/**
 * A wall standing on the floor plane, facing the camera.
 *  back          stands on the line y, running along x (content upright)
 *  left          stands on the line x, running along y (colour only; content sideways)
 *  left-upright  stands on the line x, running from y back towards y - length (content upright)
 */
type WallAxis = "back" | "left" | "left-upright";

interface WallProps {
  readonly axis: WallAxis;
  readonly x: number;
  readonly y: number;
  readonly z: number;
  readonly length: number;
  readonly height: number;
  readonly background: string;
  readonly className?: string;
  readonly children?: ReactNode;
}

export function Wall({ axis, x, y, z, length, height, background, className, children }: WallProps): React.ReactElement {
  let place: CSSProperties;
  switch (axis) {
    case "back":
      place = { left: x, top: y - height, width: length, height, transformOrigin: "50% 100%", transform: `translateZ(${z}px) rotateX(-90deg)` };
      break;
    case "left":
      place = { left: x - height, top: y, width: height, height: length, transformOrigin: "100% 50%", transform: `translateZ(${z}px) rotateY(90deg)` };
      break;
    case "left-upright":
      place = { left: x, top: y - height, width: length, height, transformOrigin: "0 100%", transform: `translateZ(${z}px) rotateZ(-90deg) rotateX(-90deg)` };
      break;
  }
  return (
    <div className={cx("iso-wall", className)} style={{ ...place, background }}>
      {children}
    </div>
  );
}

interface BillboardProps {
  readonly x: number;
  readonly y: number;
  readonly z?: number;
  readonly className?: string;
  readonly children: ReactNode;
}

/** Flat content anchored at a world point, always facing the camera. */
export function Billboard({ x, y, z = 0, className, children }: BillboardProps): React.ReactElement {
  const cam = useContext(CameraContext);
  return (
    <div className={cx("billboard", className)} style={{ left: x, top: y, transform: billboardTransform(cam, z) }}>
      {children}
    </div>
  );
}
