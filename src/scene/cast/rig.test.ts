import { describe, expect, it } from "vitest";
import { BUILDS, TIER_FAMILIES, TIER_WORKERS } from "../../lib/modelTiers";
import { lookOf } from "../../lib/looks";
import { AYO, BOSS, DEV, FERN, MARA, THEO, WALT } from "./parts";
import { rig } from "./rig";
import type { PersonParts, Placed, Rig } from "./rig";

type Point = readonly [number, number];

const pivot = (at: Placed, x = 0, y = 0): Point => [x + at.left + at.ox, y + at.top + at.oy];

/** Every joint of a rig in the puppet's space (origin: the seat, or the feet when standing). */
function joints(r: Rig, parts: PersonParts): Record<string, Point> {
  const ox = r.torso.left;
  const oy = r.torso.top;
  const [armA, armB] = r.upperArm;
  const [foreA, foreB] = r.forearm;
  const [legA, legB] = r.leg;
  const px = ox + r.pelvis.left;
  const py = oy + r.pelvis.top;
  return {
    torsoTop: [r.torso.left + parts.torso.w / 2, r.torso.top],
    neck: pivot(r.head, ox, oy),
    shoulderA: pivot(armA, ox, oy),
    shoulderB: pivot(armB, ox, oy),
    elbowA: pivot(foreA, ox + armA.left, oy + armA.top),
    elbowB: pivot(foreB, ox + armB.left, oy + armB.top),
    pelvisTop: [px + r.pelvis.w / 2, py],
    hipA: pivot(legA, px, py),
    hipB: pivot(legB, px, py),
    chairFoot: pivot(r.chair, ox, oy),
  };
}

function expectClose(actual: Record<string, Point>, expected: Record<string, Point>): void {
  expect(Object.keys(actual)).toEqual(Object.keys(expected));
  for (const [name, [x, y]] of Object.entries(actual)) {
    const [ex, ey] = expected[name] ?? [Number.NaN, Number.NaN];
    expect(x, `${name} x`).toBeCloseTo(ex, 6);
    expect(y, `${name} y`).toBeCloseTo(ey, 6);
  }
}

const flat = (r: Rig): readonly Placed[] => [r.torso, r.head, r.chair, r.pelvis, ...r.upperArm, ...r.forearm, ...r.leg];

function expectSameRig(actual: Rig, expected: Rig): void {
  const [a, e] = [flat(actual), flat(expected)];
  a.forEach((box, i) => {
    const want = e[i];
    for (const key of ["left", "top", "w", "h", "ox", "oy"] as const) expect(box[key], `${i} ${key}`).toBeCloseTo(want?.[key] ?? Number.NaN, 6);
  });
}

const tierLook = (worker: (typeof TIER_WORKERS)[number], build: (typeof BUILDS)[number]) => lookOf({ id: "x", model: TIER_FAMILIES[worker][0], effort: build });

describe("rig with a frame", () => {
  describe.each(TIER_WORKERS)("%s", (worker) => {
    it.each([false, true])("keeps every joint where the medium build has it in all six builds (standing: %s)", (standing) => {
      const medium = tierLook(worker, "medium");
      const at = joints(rig(medium.parts, standing, medium.frame), medium.parts);
      for (const build of BUILDS) {
        const look = tierLook(worker, build);
        expectClose(joints(rig(look.parts, standing, look.frame), look.parts), at);
      }
    });

    it.each([false, true])("moves the shoulders between minimal and max without the frame (standing: %s)", (standing) => {
      const lean = tierLook(worker, "minimal").parts;
      const buff = tierLook(worker, "max").parts;
      expect(joints(rig(lean, standing), lean).shoulderA).not.toEqual(joints(rig(buff, standing), buff).shoulderA);
    });
  });
});

/** The rig before frames, written out: every joint from the drawn pieces' own sizes. */
function oldRig(parts: PersonParts, standing: boolean): Rig {
  const pin = (part: { w: number; h: number }, px: number, py: number, x: number, y: number): Placed => ({ left: x - part.w * px, top: y - part.h * py, w: part.w, h: part.h, ox: part.w * px, oy: part.h * py });
  const t = parts.torso;
  const p = parts.pelvis;
  const ua = parts["upper-arm-r"];
  const ul = parts["upper-arm-l"];
  const pelvisTop = t.h * 0.84;
  const anchor = standing ? pelvisTop + p.h * 0.72 + parts["leg-r"].h * 0.97 : pelvisTop + p.h * 0.5 + t.h * 0.42;
  const shoulder = t.w * 0.38;
  const chairW = parts.chair.w * 1.7;
  const chairH = parts.chair.h * 1.7;
  return {
    torso: pin(t, 0.5, 0, 0, -anchor),
    head: pin(parts.head, 0.5, 0.86, t.w / 2, t.h * 0.1),
    chair: { left: (t.w - chairW) / 2, top: t.h * 0.02, w: chairW, h: chairH, ox: chairW / 2, oy: chairH },
    pelvis: pin(p, 0.5, 0, t.w / 2, pelvisTop),
    upperArm: [pin(ua, 0.5, 0.12, t.w / 2 - shoulder, t.h * 0.18), pin(ul, 0.5, 0.12, t.w / 2 + shoulder, t.h * 0.18)],
    forearm: [pin(parts["forearm-r"], 0.5, 0.05, ua.w / 2, ua.h * 0.9), pin(parts["forearm-l"], 0.5, 0.05, ul.w / 2, ul.h * 0.9)],
    leg: [pin(parts["leg-r"], 0.5, 0.04, p.w * 0.27, p.h * 0.72), pin(parts["leg-l"], 0.5, 0.04, p.w * 0.73, p.h * 0.72)],
  };
}

describe("rig for the first cast", () => {
  const cast = { THEO, MARA, WALT, AYO, DEV, FERN, BOSS };
  describe.each(Object.entries(cast))("%s", (_name, parts) => {
    it.each([false, true])("places every piece as before frames (standing: %s)", (standing) => {
      expectSameRig(rig(parts, standing), oldRig(parts, standing));
    });

    it.each([false, true])("is the same with itself as the frame (standing: %s)", (standing) => {
      expect(rig(parts, standing, parts)).toEqual(rig(parts, standing));
    });
  });
});
