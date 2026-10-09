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

export function rig(parts: PersonParts, standing: boolean): Rig {
  const t = parts.torso;
  const p = parts.pelvis;
  const ua = parts["upper-arm-r"];
  const leg = parts["leg-r"];
  const pelvisTop = t.h * 0.84;
  // Seated, the desk hides the lap: sitting tall puts the hands at its back edge, on the keyboard.
  const anchor = standing ? pelvisTop + p.h * 0.72 + leg.h * 0.97 : pelvisTop + p.h * 0.5 + t.h * SIT_TALL;
  const shoulder = t.w * 0.38;
  const chairW = parts.chair.w * CHAIR_SCALE;
  const chairH = parts.chair.h * CHAIR_SCALE;
  return {
    torso: pin(t, 0.5, 0, 0, -anchor),
    head: pin(parts.head, 0.5, 0.86, t.w / 2, t.h * 0.1),
    chair: { left: (t.w - chairW) / 2, top: t.h * 0.02, w: chairW, h: chairH, ox: chairW / 2, oy: chairH },
    pelvis: pin(p, 0.5, 0, t.w / 2, pelvisTop),
    upperArm: [pin(ua, 0.5, 0.12, t.w / 2 - shoulder, t.h * 0.18), pin(parts["upper-arm-l"], 0.5, 0.12, t.w / 2 + shoulder, t.h * 0.18)],
    forearm: [pin(parts["forearm-r"], 0.5, 0.05, ua.w / 2, ua.h * 0.9), pin(parts["forearm-l"], 0.5, 0.05, parts["upper-arm-l"].w / 2, parts["upper-arm-l"].h * 0.9)],
    leg: [pin(leg, 0.5, 0.04, p.w * 0.27, p.h * 0.72), pin(parts["leg-l"], 0.5, 0.04, p.w * 0.73, p.h * 0.72)],
  };
}
