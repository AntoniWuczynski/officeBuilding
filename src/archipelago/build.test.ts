import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { WOODS } from "../lib/woods";
import type { Wood } from "../lib/woods";
import type { ToolKind } from "../types";
import { COUNTER_H, buildStall } from "./build";

/** The counter is the one mesh as tall as COUNTER_H. */
function counterColor(tool: ToolKind): number {
  const group = buildStall(false, 1, tool);
  for (const child of group.children) {
    if (child instanceof THREE.Mesh && child.geometry instanceof THREE.BoxGeometry && child.geometry.parameters.height === COUNTER_H && child.material instanceof THREE.MeshStandardMaterial) {
      return child.material.color.getHex();
    }
  }
  throw new Error("no counter");
}

const hex = (w: Wood): number => new THREE.Color(WOODS[w].top).getHex();

describe("a hut's counter", () => {
  it("takes the tool's wood", () => {
    expect(counterColor("codex")).toBe(hex("wenge"));
    expect(counterColor("claude-code")).toBe(hex("maple"));
    expect(counterColor("cursor")).toBe(hex("oak"));
  });
});
