import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { selectors } from "./cssSelectors";

// The island stylesheet stays in the page once Islands has been opened, so a bare selector here restyles the office
// (a bare .is-dim once flattened dimmed storeys). Every rule must be scoped to the island view.
const SCOPED = /^(\.arch[\w-]*|\.building-body\.is-islands|\.app\.is-islands)(?![\w-])/;

describe("archipelago.css", () => {
  it("scopes every rule to the island view", () => {
    const css = readFileSync("src/archipelago/archipelago.css", "utf8");
    const loose = selectors(css).filter((s) => !SCOPED.test(s));
    expect(loose).toEqual([]);
  });

  it("would catch a bare selector", () => {
    expect(selectors(".arch-list { a: b } .is-dim { opacity: 0.35 }").filter((s) => !SCOPED.test(s))).toEqual([".is-dim"]);
  });

  it("would catch an at-rule", () => {
    expect(selectors("@keyframes sway { to { opacity: 0 } } .arch-list { a: b }").filter((s) => !SCOPED.test(s))).toEqual(["@keyframes sway", "to"]);
  });
});
