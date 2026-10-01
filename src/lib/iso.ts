// Geometry for the CSS 3D doll's-house rig (DESIGN.md, implementation
// constraints). A "world" is a W×D floor-plane element rotated by
// rotateX(tilt) rotateZ(rotZ); +z points up. Billboards undo the rotation so
// people and signs always face the camera.

export interface Camera {
  /** Rotation around the vertical axis, degrees. */
  readonly rotZ: number;
  /** Tilt of the floor plane away from the viewer, degrees. */
  readonly tilt: number;
}

/** The building exterior: near-frontal, eye-level, so storeys stay readable. */
export const BUILDING_CAMERA: Camera = { rotZ: 18, tilt: 68 };
/** Inside one floor: classic isometric, no ceiling. */
export const FLOOR_CAMERA: Camera = { rotZ: 45, tilt: 60 };

export interface ScreenPoint {
  readonly x: number;
  readonly y: number;
}

/** A point on the floor plane, in world pixels. */
export interface PlanPoint {
  readonly x: number;
  readonly y: number;
}

export interface Bounds {
  readonly minX: number;
  readonly maxX: number;
  readonly minY: number;
  readonly maxY: number;
}

const rad = (deg: number): number => (deg * Math.PI) / 180;

/** Screen offset of world point (x, y, z) from the projected centre of a W×D world. */
export function project(
  cam: Camera,
  W: number,
  D: number,
  x: number,
  y: number,
  z: number,
): ScreenPoint {
  const dx = x - W / 2;
  const dy = y - D / 2;
  const r = rad(cam.rotZ);
  const t = rad(cam.tilt);
  const x1 = dx * Math.cos(r) - dy * Math.sin(r);
  const y1 = dx * Math.sin(r) + dy * Math.cos(r);
  return { x: x1, y: y1 * Math.cos(t) - z * Math.sin(t) };
}

/** Screen-space bounds of the box [0,W]×[0,D]×[0,zTop], relative to the world centre. */
export function projectedBounds(cam: Camera, W: number, D: number, zTop: number): Bounds {
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const x of [0, W]) {
    for (const y of [0, D]) {
      for (const z of [0, zTop]) {
        const p = project(cam, W, D, x, y, z);
        minX = Math.min(minX, p.x);
        maxX = Math.max(maxX, p.x);
        minY = Math.min(minY, p.y);
        maxY = Math.max(maxY, p.y);
      }
    }
  }
  return { minX, maxX, minY, maxY };
}

export function worldTransform(cam: Camera): string {
  return `rotateX(${cam.tilt}deg) rotateZ(${cam.rotZ}deg)`;
}

/** Stand a flat element at height z, facing the camera. */
export function billboardTransform(cam: Camera, z: number): string {
  return `translateZ(${z}px) rotateZ(${-cam.rotZ}deg) rotateX(${-cam.tilt}deg)`;
}

/** Whether moving from a to b travels leftwards on screen (walkers face that way). */
export function headsLeft(cam: Camera, W: number, D: number, from: PlanPoint, to: PlanPoint): boolean {
  return project(cam, W, D, to.x, to.y, 0).x < project(cam, W, D, from.x, from.y, 0).x;
}
