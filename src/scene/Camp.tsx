import { useContext } from "react";
import { DESK, doorOf, seatOf } from "../lib/layout";
import { FLOOR_CAMERA, project, projectedBounds } from "../lib/iso";
import type { Camera } from "../lib/iso";
import type { Look } from "../lib/looks";
import { BOSS, DEV } from "./cast/parts";
import { Billboard, CameraContext, TAG_TOP, heightToClear } from "./Iso";

/** The overview's hut and palm colours (archipelago/build.ts M), with darker shades for faces turned from the light. */
export const CAMP = {
  wood: "#b9824a",
  woodFront: "#9c6b3a",
  woodSide: "#835a30",
  wall: "#6e4628",
  wallSide: "#573620",
  roof: "#d9534f",
  roofSide: "#b23f3c",
  thatch: "#e0bd6a",
  thatchSide: "#b9964b",
  trunk: "#8a5a33",
  leaf: "#2f9e44",
  leafShade: "#24803a",
  sand: "#f0d9a0",
} as const;

/** The manager on an island: the boss's face in a light blue shirt, rolled sleeves, no suit. */
export const ISLAND_MANAGER_LOOK: Look = {
  name: "boss",
  parts: { ...BOSS, torso: DEV.torso, pelvis: DEV.pelvis, "upper-arm-l": DEV["upper-arm-l"], "upper-arm-r": DEV["upper-arm-r"], "forearm-l": DEV["forearm-l"], "forearm-r": DEV["forearm-r"], "leg-l": DEV["leg-l"], "leg-r": DEV["leg-r"] },
};

const POST = 3;
/** The back wall's front face stands this far behind the crate, behind the worker (seatOf: 2 behind). */
const BACK = 12;
const WALL = 3;
/** The roof's square base, from the crate's top-left corner: it reaches over the back wall and past both posts. */
const ROOF = { x: -POST - 3, y: -BACK - WALL - 2, size: DESK.w + 2 * (POST + 3), rise: 26 } as const;
/**
 * Eave height over the crate's foot: the roof's front-right corner is its lowest point over the worker's name tag on
 * screen, and it stays 4px above the top of that tag.
 */
const EAVE = heightToClear(FLOOR_CAMERA, { ...seatOf({ x: 0, y: 0 }), z: 0 }, { x: ROOF.x + ROOF.size, y: ROOF.y + ROOF.size }, TAG_TOP.name + 4);

type P3 = readonly [number, number, number];

/** The front and right faces of the box [x0, x1] x [y0, y1] x [0, h]: what the floor camera sees of it. */
function boxFaces(x0: number, x1: number, y0: number, y1: number, h: number): readonly [readonly P3[], readonly P3[]] {
  return [
    [[x0, y1, 0], [x1, y1, 0], [x1, y1, h], [x0, y1, h]],
    [[x1, y1, 0], [x1, y0, 0], [x1, y0, h], [x1, y1, h]],
  ];
}

interface HutProps {
  /** The crate's top-left corner, as a desk slot. */
  readonly x: number;
  readonly y: number;
  readonly z: number;
  /** The manager's hut is thatched. */
  readonly thatch?: boolean;
}

/**
 * The overview's hut over a crate: a dark back wall behind the worker, a post at each end and a four-sided roof.
 * It is one picture of those parts in the floor's projection, standing at the back wall: as solid boxes the tall wall
 * and posts leaned nearer the camera than the worker's name tag in front of them, and cut through it.
 */
export function Hut({ x, y, z, thatch = false }: HutProps): React.ReactElement {
  const cam = useContext(CameraContext);
  const foot = { x: x + DESK.w / 2, y: y - BACK };
  const origin = project(cam, 0, 0, foot.x, foot.y, 0);
  const screen = ([px, py, pz]: P3): readonly [number, number] => {
    const p = project(cam, 0, 0, x + px, y + py, pz);
    return [p.x - origin.x, p.y - origin.y];
  };
  const [rx0, ry0] = [ROOF.x, ROOF.y];
  const [rx1, ry1] = [ROOF.x + ROOF.size, ROOF.y + ROOF.size];
  const apex: P3 = [(rx0 + rx1) / 2, (ry0 + ry1) / 2, EAVE + ROOF.rise];
  const [wallFront, wallSide] = boxFaces(-POST, DESK.w + POST, -BACK - WALL, -BACK, EAVE);
  const posts = [-POST, DESK.w].map((px) => boxFaces(px, px + POST, DESK.d - POST, DESK.d, EAVE));
  const faces: readonly (readonly [readonly P3[], string])[] = [
    [wallFront, CAMP.wall],
    [wallSide, CAMP.wallSide],
    ...posts.flatMap(([front, side]) => [[front, CAMP.trunk], [side, CAMP.wallSide]] as const),
    [[[rx0, ry1, EAVE], [rx1, ry1, EAVE], apex], thatch ? CAMP.thatch : CAMP.roof],
    [[[rx1, ry1, EAVE], [rx1, ry0, EAVE], apex], thatch ? CAMP.thatchSide : CAMP.roofSide],
  ];
  const drawn = faces.map(([points, fill]) => [points.map(screen), fill] as const);
  const xs = drawn.flatMap(([pts]) => pts.map(([px]) => px));
  const ys = drawn.flatMap(([pts]) => pts.map(([, py]) => py));
  const box = { x: Math.min(...xs), y: Math.min(...ys), w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys) };
  return (
    <div className="group hut" data-testid={thatch ? "manager-hut" : "hut"}>
      <Billboard x={foot.x} y={foot.y} z={z}>
        <svg viewBox={`${box.x} ${box.y} ${box.w} ${box.h}`} width={box.w} height={box.h} style={{ position: "absolute", left: box.x, top: box.y }} aria-hidden>
          {drawn.map(([pts, fill], i) => (
            <polygon key={i} points={pts.map((p) => p.join(",")).join(" ")} fill={fill} />
          ))}
        </svg>
      </Billboard>
    </div>
  );
}

/** A palm standing on the sand at (x, y). */
function Palm({ x, y }: { readonly x: number; readonly y: number }): React.ReactElement {
  return (
    <Billboard x={x} y={y}>
      <svg data-testid="camp-palm" viewBox="0 0 60 90" width="60" height="90" style={{ position: "absolute", left: -29, bottom: 0, pointerEvents: "none" }} aria-hidden>
        <path d="M29 90 Q25 56 33 22" stroke={CAMP.trunk} strokeWidth="5" strokeLinecap="round" fill="none" />
        {[150, 195, 30, -15, -60, -115].map((a, i) => (
          <ellipse key={a} cx="33" cy="22" rx="16" ry="4.5" fill={i % 2 === 0 ? CAMP.leaf : CAMP.leafShade} transform={`rotate(${a} 33 22) translate(14 0)`} />
        ))}
      </svg>
    </Billboard>
  );
}

/** How far out from each room corner, along both axes, its palm stands. */
const PALM_OUT = 18;
/** A palm's foot stands this far inside the surf. */
const SHORE = 14;
const SURF = 6;

/**
 * Where the palms stand: off each corner of the room except the open one by the door. The sign-out stand stands there,
 * and a palm drawn in front of it covered its sign.
 */
function palmSpots(width: number, depth: number): readonly (readonly [number, number])[] {
  const door = doorOf(depth);
  const corners: readonly (readonly [number, number])[] = [
    [-PALM_OUT, -PALM_OUT],
    [width + PALM_OUT, -PALM_OUT],
    [-PALM_OUT, depth + PALM_OUT],
    [width + PALM_OUT, depth + PALM_OUT],
  ];
  const away = ([x, y]: readonly [number, number]): number => Math.hypot(x - door.x, y - door.y);
  const open = Math.min(...corners.map(away));
  return corners.filter((c) => away(c) !== open);
}

/** Radius of the sand round a W x D room: past its corner palms. */
function beachRadius(width: number, depth: number): number {
  return Math.hypot(width / 2 + PALM_OUT, depth / 2 + PALM_OUT) + SHORE;
}

/** Screen room the sand and its surf need either side of a W x D room's corners: a disc of radius r is 2r wide on screen. */
export function beachSidePad(cam: Camera, width: number, depth: number): number {
  const b = projectedBounds(cam, width, depth, 0);
  return beachRadius(width, depth) + SURF - (b.maxX - b.minX) / 2;
}

/**
 * The island under an open-air room: a disc of sand ringed with surf and a pale lagoon, on the floor plane so it zooms
 * and pans with the room, and a palm off each corner but the door's.
 */
export function Beach({ width, depth }: { readonly width: number; readonly depth: number }): React.ReactElement {
  const r = beachRadius(width, depth);
  const lagoon = r * 1.5;
  const disc = (radius: number): React.CSSProperties => ({
    position: "absolute",
    left: width / 2 - radius,
    top: depth / 2 - radius,
    width: 2 * radius,
    height: 2 * radius,
    borderRadius: "50%",
    pointerEvents: "none",
  });
  return (
    <>
      <div
        style={{ ...disc(lagoon), transform: "translateZ(-1px)", background: "radial-gradient(closest-side, rgba(155, 232, 226, 0.95) 60%, rgba(155, 232, 226, 0))" }}
      />
      <div data-testid="beach" style={{ ...disc(r), background: CAMP.sand, boxShadow: `0 0 0 ${SURF}px #fff` }} />
      {palmSpots(width, depth).map(([px, py]) => (
        <Palm key={`${px},${py}`} x={px} y={py} />
      ))}
    </>
  );
}
