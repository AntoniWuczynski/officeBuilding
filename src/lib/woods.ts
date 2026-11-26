import type { ToolKind } from "../types";

export type Wood = "maple" | "wenge" | "oak" | "walnut" | "cherry" | "pine";

/** The desk wood of each tool (HUMAN_DECISIONS OB-335): walnut for Claude Code and oak for Codex (A Human, 2026-11-23). */
export const TOOL_WOOD: Readonly<Record<ToolKind, Wood>> = {
  "claude-code": "walnut",
  codex: "oak",
  cursor: "wenge",
  opencode: "maple",
  antigravity: "cherry",
  unknown: "pine",
};

export interface WoodFaces {
  /** In words, for the legend. */
  readonly label: string;
  /** Flat colours of the top, front and side faces: the fallback under the grain, and what tests assert. */
  readonly top: string;
  readonly front: string;
  readonly side: string;
  /** The grain tile (art/wood/tile.py). */
  readonly tile: string;
}

export const WOODS: Readonly<Record<Wood, WoodFaces>> = {
  maple: { label: "Pale maple", top: "#f1c080", front: "#bc9664", side: "#95774f", tile: new URL("../assets/wood/maple.jpg", import.meta.url).href },
  pine: { label: "Pine", top: "#f2ba62", front: "#bd914c", side: "#96733d", tile: new URL("../assets/wood/pine.jpg", import.meta.url).href },
  oak: { label: "Oak", top: "#b26e2a", front: "#8b5621", side: "#6e441a", tile: new URL("../assets/wood/oak.jpg", import.meta.url).href },
  cherry: { label: "Cherry", top: "#9f4115", front: "#7c3310", side: "#63280d", tile: new URL("../assets/wood/cherry.jpg", import.meta.url).href },
  walnut: { label: "Walnut", top: "#4d2b1a", front: "#3c2214", side: "#301b10", tile: new URL("../assets/wood/walnut.jpg", import.meta.url).href },
  wenge: { label: "Wenge", top: "#1b0e0a", front: "#150b08", side: "#110906", tile: new URL("../assets/wood/wenge.jpg", import.meta.url).href },
};

/** The tile is 192 px, drawn at this size so it stays sharp at the floor's zoom. */
const TILE_PX = 48;

export function woodFor(tool: ToolKind): Wood {
  return TOOL_WOOD[tool];
}

/** The three `background` values of a desk in this wood, for IsoBox. */
export function deskFaces(wood: Wood): { readonly top: string; readonly front: string; readonly side: string } {
  const { top, front, side, tile } = WOODS[wood];
  return { top: `${top} url("${tile}") 0 0 / ${TILE_PX}px ${TILE_PX}px`, front, side };
}
