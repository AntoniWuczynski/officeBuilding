import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { selectors } from "./cssSelectors";

/** Selectors not under .arch-floor. */
const loose = (css: string): string[] => selectors(css).filter((s) => !/^\.arch-floor(?![\w-])/.test(s));

/** --room-* tokens `css` sets that `readers` never read with a fallback. */
function unread(css: string, readers: string): string[] {
  const tokens = [...css.matchAll(/(--room-[\w-]+)\s*:/g)].map((m) => m[1] ?? "");
  return tokens.filter((t) => !new RegExp(`var\\(${t}, var\\(`).test(readers));
}

// camp.css loads on every page, so a bare selector in it would restyle the office floor: every rule stays under .arch-floor.
describe("camp.css", () => {
  const css = readFileSync("src/archipelago/camp.css", "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

  it("scopes every rule to the island floor", () => {
    expect(loose(css)).toEqual([]);
  });

  it("would catch a bare selector and an at-rule", () => {
    expect(loose(".arch-floor .a { b: c } .pp-chair { d: e } @media (width > 1px) { .arch-floor .f { g: h } }")).toEqual([".pp-chair", "@media (width > 1px)"]);
  });

  it("sets only tokens the office reads with a fallback", () => {
    const rooms = ["src/scene/materials.ts", "src/components/floor/Room.tsx"].map((f) => readFileSync(f, "utf8")).join("\n");
    expect(css).toMatch(/--room-[\w-]+\s*:/);
    expect(unread(css, rooms)).toEqual([]);
  });

  it("would catch a token the office reads without a fallback", () => {
    expect(unread(".arch-floor { --room-sky: blue; --room-sand: tan }", `"var(--room-sky)" "var(--room-sand, var(--boards))"`)).toEqual(["--room-sky"]);
  });
});
