import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// The island stylesheet stays in the page once Islands has been opened, so a bare selector here restyles the office
// (a bare .is-dim once flattened dimmed storeys). Every rule must be scoped to the island view.
const SCOPED = /^(\.arch[\w-]*|\.building-body\.is-islands|\.app\.is-islands)(?![\w-])/;

/** Top-level selectors of a stylesheet, comments stripped. */
export function selectors(css: string): string[] {
  const body = css.replace(/\/\*[\s\S]*?\*\//g, "");
  return [...body.matchAll(/([^{}]+)\{/g)].flatMap((m) => (m[1] ?? "").split(",").map((s) => s.trim())).filter((s) => s !== "");
}

describe("archipelago.css", () => {
  it("scopes every rule to the island view", () => {
    const css = readFileSync("src/archipelago/archipelago.css", "utf8");
    const loose = selectors(css).filter((s) => !SCOPED.test(s));
    expect(loose).toEqual([]);
  });

  it("would catch a bare selector", () => {
    expect(selectors(".arch-list { a: b } .is-dim { opacity: 0.35 }").filter((s) => !SCOPED.test(s))).toEqual([".is-dim"]);
  });
});
