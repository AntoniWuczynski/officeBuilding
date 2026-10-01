import { createContext, useContext } from "react";
import type { CSSProperties, ReactNode } from "react";
import { FLOOR_CAMERA, billboardTransform } from "../lib/iso";
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
