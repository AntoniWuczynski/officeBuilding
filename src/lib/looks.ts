// Each employee gets a stable look derived from their session id, so the same
// agent is recognisable on the building and inside the floor.

export interface Look {
  readonly shirt: string;
  readonly skin: string;
  readonly hair: string;
}

type Palette = readonly [string, ...string[]];

const SHIRTS: Palette = ["#2E7D7A", "#C9A227", "#B5532B", "#4A5A73", "#6B4A6E", "#3F7A3A", "#8A5A9E", "#A64452"];
const SKINS: Palette = ["#F1C9A5", "#C68E62", "#8D5A3B", "#E8B894", "#A86F4C", "#5C3A26"];
const HAIRS: Palette = ["#2B1D14", "#5A3A22", "#1B1B1B", "#A0522D", "#D9C27A", "#7A7068"];

/** The manager's look: fixed, so the corner office is always the same person. */
export const MANAGER_LOOK: Look = { shirt: "#3A3F4B", skin: "#D7A57E", hair: "#4A3A2A" };

/** FNV-1a, 32-bit. */
export function hashString(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function pick(palette: Palette, n: number): string {
  return palette[n % palette.length] ?? palette[0];
}

export function lookFor(id: string): Look {
  const h = hashString(id);
  return {
    shirt: pick(SHIRTS, h),
    skin: pick(SKINS, h >>> 8),
    hair: pick(HAIRS, h >>> 16),
  };
}
