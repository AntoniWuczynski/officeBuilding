import { describe, expect, it } from "vitest";
import type { ToolKind } from "../types";
import { TOOL_WOOD, WOODS, deskFaces, woodFor } from "./woods";
import type { Wood } from "./woods";

const TOOLS: readonly ToolKind[] = ["claude-code", "codex", "cursor", "opencode", "antigravity", "unknown"];

const luma = (hex: string): number => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  return 0.299 * (r ?? 0) + 0.587 * (g ?? 0) + 0.114 * (b ?? 0);
};

describe("Desk wood per ToolKind", () => {
  it("gives each tool its wood", () => {
    expect(TOOLS.map((t) => [t, woodFor(t)])).toEqual([
      ["claude-code", "maple"],
      ["codex", "wenge"],
      ["cursor", "oak"],
      ["opencode", "walnut"],
      ["antigravity", "cherry"],
      ["unknown", "pine"],
    ]);
    expect(TOOL_WOOD["claude-code"]).toBe("maple");
  });

  it("keeps the flat colours of the table", () => {
    const flat = Object.entries(WOODS).map(([w, f]) => [w, f.label, f.top, f.front, f.side]);
    expect(flat).toEqual([
      ["maple", "Pale maple", "#f1c080", "#bc9664", "#95774f"],
      ["pine", "Pine", "#f2ba62", "#bd914c", "#96733d"],
      ["oak", "Oak", "#b26e2a", "#8b5621", "#6e441a"],
      ["cherry", "Cherry", "#9f4115", "#7c3310", "#63280d"],
      ["walnut", "Walnut", "#4d2b1a", "#3c2214", "#301b10"],
      ["wenge", "Wenge", "#1b0e0a", "#150b08", "#110906"],
    ]);
  });

  it("orders the tops by lightness, Claude Code lightest and Codex darkest", () => {
    const order: readonly Wood[] = ["maple", "pine", "oak", "cherry", "walnut", "wenge"];
    const lumas = order.map((w) => luma(WOODS[w].top));
    expect([...lumas].sort((a, b) => b - a)).toEqual(lumas);
    expect(Math.max(...lumas)).toBe(luma(WOODS[woodFor("claude-code")].top));
    expect(Math.min(...lumas)).toBe(luma(WOODS[woodFor("codex")].top));
  });

  it("names a tile for every wood", () => {
    for (const w of Object.values(TOOL_WOOD)) expect(WOODS[w].tile).toContain(`${w}.jpg`);
  });

  it("deskFaces keeps the flat front and side and puts the tile over the flat top", () => {
    for (const w of Object.values(TOOL_WOOD)) {
      const faces = deskFaces(w);
      expect(faces.front).toBe(WOODS[w].front);
      expect(faces.side).toBe(WOODS[w].side);
      expect(faces.top.startsWith(WOODS[w].top)).toBe(true);
      expect(faces.top).toContain(WOODS[w].tile);
    }
  });
});
