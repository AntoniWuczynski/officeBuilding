// Which person a click lands on. The figures are page elements laid over the canvas (pointer-events: none, so dragging
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
