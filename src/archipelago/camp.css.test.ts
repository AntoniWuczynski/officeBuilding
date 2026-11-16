import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// camp.css loads on every page, so a bare selector in it would restyle the office floor: every rule stays under .arch-floor.
describe("camp.css", () => {
  const css = readFileSync("src/archipelago/camp.css", "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const selectors = [...css.matchAll(/([^{}]+)\{/g)].flatMap((m) => (m[1] ?? "").split(",").map((s) => s.trim())).filter((s) => s !== "");

  it("scopes every rule to the island floor", () => {
    expect(selectors.filter((s) => !/^\.arch-floor(?![\w-])/.test(s))).toEqual([]);
  });

  it("sets only tokens the office reads with a fallback", () => {
    const tokens = [...css.matchAll(/(--room-[\w-]+)\s*:/g)].map((m) => m[1]);
    const rooms = ["src/scene/materials.ts", "src/components/floor/Room.tsx"].map((f) => readFileSync(f, "utf8")).join("\n");
    expect(tokens.length).toBeGreaterThan(0);
    for (const t of tokens) expect(rooms, t).toMatch(new RegExp(`var\\(${t}, var\\(`));
  });
});
