// Screen geometry for the island map: which person a click lands on, and how much room the floating chrome leaves.
// The figures are page elements laid over the canvas (pointer-events: none, so dragging
// and scrolling over them still pans and zooms the map), so a click is hit-tested against their boxes here.

export interface Box {
  readonly key: string;
  readonly left: number;
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
  /** The figure's stacking order: the loop draws the one lower on screen on top. */
  readonly z: number;
}

/** The topmost box holding the point. A box with no area (a hidden figure) never matches. */
export function boxAt(boxes: readonly Box[], x: number, y: number): string | undefined {
  let best: Box | undefined;
  for (const b of boxes) {
    if (b.right <= b.left || b.bottom <= b.top) continue;
    if (x < b.left || x > b.right || y < b.top || y > b.bottom) continue;
    if (best === undefined || b.z > best.z) best = b;
  }
  return best?.key;
}

export interface Rect {
  readonly left: number;
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
}

const overlaps = (a: Rect, b: Rect): boolean => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;

/**
 * The box inside `host`, clear of every `cover` (and `gap` from each), that fits content `w` by `h` at the largest
 * scale: the room the floating list, usage signs and island panel leave. Every free edge is a host edge or a cover
 * edge, so the candidates are few and checked in full. Null when nothing is free.
 */
export function roomFor(host: Rect, covers: readonly Rect[], gap: number, w: number, h: number): Rect | null {
  const lefts = [host.left + gap, ...covers.map((c) => c.right + gap)];
  const rights = [host.right - gap, ...covers.map((c) => c.left - gap)];
  const tops = [host.top + gap, ...covers.map((c) => c.bottom + gap)];
  const bottoms = [host.bottom - gap, ...covers.map((c) => c.top - gap)];
  const grown = covers.map((c) => ({ left: c.left - gap, top: c.top - gap, right: c.right + gap, bottom: c.bottom + gap }));
  let best: Rect | null = null;
  let bestScale = 0;
  for (const left of lefts)
    for (const right of rights)
      for (const top of tops)
        for (const bottom of bottoms) {
          if (right <= left || bottom <= top) continue;
          const box = { left, top, right, bottom };
          if (grown.some((c) => overlaps(box, c))) continue;
          const scale = Math.min((right - left) / w, (bottom - top) / h);
          if (scale > bestScale) {
            best = box;
            bestScale = scale;
          }
        }
  return best;
}
