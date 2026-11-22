// Where a cut-out person's parts sit and pivot, from the parts' own sizes, so
// every character fits one rig. Units are the cut sheets' pixels (every torso
// is 96 tall, art/cast/cut.py); the puppet scales them down as a whole.

import type { PartSize } from "./parts";

export type PersonPart =
  | "head"
  | "head-closed"
  | "torso"
  | "chair"
  | "upper-arm-l"
  | "upper-arm-r"
  | "forearm-l"
  | "forearm-r"
  | "leg-l"
  | "leg-r"
  | "pelvis";

export interface Part extends PartSize {
  readonly src: string;
}

export type PersonParts = Readonly<Record<PersonPart, Part>>;

/** A part's box inside its parent's box, and the point it turns about (in its own box). */
export interface Placed {
  readonly left: number;
  readonly top: number;
  readonly w: number;
  readonly h: number;
  readonly ox: number;
  readonly oy: number;
}

export interface Rig {
  /** The torso inside the puppet, whose (0, 0) is the seat (seated) or the feet (standing). */
  readonly torso: Placed;
  /** These sit inside the torso's box. */
  readonly head: Placed;
  readonly chair: Placed;
  readonly pelvis: Placed;
  /** Screen-left and screen-right arm (the person's right and left). */
  readonly upperArm: readonly [Placed, Placed];
  /** Inside their upper arm's box. */
  readonly forearm: readonly [Placed, Placed];
  /** Inside the pelvis's box. */
  readonly leg: readonly [Placed, Placed];
}

/** Turns a part about `(px, py)`, as shares of its own size, and puts that point at `(x, y)`. */
function pin(part: PartSize, px: number, py: number, x: number, y: number): Placed {
  const ox = part.w * px;
  const oy = part.h * py;
  return { left: x - ox, top: y - oy, w: part.w, h: part.h, ox, oy };
}

const CHAIR_SCALE = 1.7;
/** How far above the seat a seated torso is lifted, as a share of its height. */
const SIT_TALL = 0.42;

/**
 * Places a person's pieces. `frame` is the piece set whose sizes place the joints: a tier
 * worker's medium build, so its six builds share one set of joints and a build changes only
 * how wide each piece is round its joint. Each piece is still drawn at its own size.
 */
export function rig(parts: PersonParts, standing: boolean, frame: PersonParts = parts): Rig {
  const t = frame.torso;
  const p = frame.pelvis;
  const pelvisTop = t.h * 0.84;
  // Seated, the desk hides the lap: sitting tall puts the hands at its back edge, on the keyboard.
  const anchor = standing ? pelvisTop + p.h * 0.72 + frame["leg-r"].h * 0.97 : pelvisTop + p.h * 0.5 + t.h * SIT_TALL;
  const shoulder = t.w * 0.38;
  // The hips from the centre line: 0.27 and 0.73 of the pelvis's width, measured from its middle.
  const hip = p.w * 0.23;
  // The drawn torso's own centre line: its children sit inside its own box.
  const mid = parts.torso.w / 2;
  const chairW = parts.chair.w * CHAIR_SCALE;
  const chairH = parts.chair.h * CHAIR_SCALE;
  const forearm = (side: "l" | "r"): Placed => {
    const arm = parts[`upper-arm-${side}`];
    // The elbow is 0.9 down the frame's upper arm, from a shoulder pinned 0.12 down the drawn one.
    return pin(parts[`forearm-${side}`], 0.5, 0.05, arm.w / 2, arm.h * 0.12 + frame[`upper-arm-${side}`].h * 0.78);
  };
  return {
    torso: pin(parts.torso, 0.5, 0, 0, -anchor),
    head: pin(parts.head, 0.5, 0.86, mid, t.h * 0.1),
    chair: { left: mid - chairW / 2, top: t.h * 0.02, w: chairW, h: chairH, ox: chairW / 2, oy: chairH },
    pelvis: pin(parts.pelvis, 0.5, 0, mid, pelvisTop),
    upperArm: [pin(parts["upper-arm-r"], 0.5, 0.12, mid - shoulder, t.h * 0.18), pin(parts["upper-arm-l"], 0.5, 0.12, mid + shoulder, t.h * 0.18)],
    forearm: [forearm("r"), forearm("l")],
    leg: [pin(parts["leg-r"], 0.5, 0.04, parts.pelvis.w / 2 - hip, p.h * 0.72), pin(parts["leg-l"], 0.5, 0.04, parts.pelvis.w / 2 + hip, p.h * 0.72)],
  };
}
