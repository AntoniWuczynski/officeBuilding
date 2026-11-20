import { describe, expect, it } from "vitest";
import { similarNames } from "./similarNames";

describe("similarNames", () => {
  it("matches a squashed prefix", () => expect(similarNames("Alphafix", ["Alpha fixing", "Delta"])).toEqual(["Alpha fixing"]));
  it("matches case and a longer name", () => expect(similarNames("gamma", ["Gamma research"])).toEqual(["Gamma research"]));
  it("matches a plural s", () => expect(similarNames("Deltas", ["Delta"])).toEqual(["Delta"]));
  it("matches spacing and hyphens", () => expect(similarNames("alpha-fix", ["Alpha fix"])).toEqual(["Alpha fix"]));
  it("matches a word in common", () => expect(similarNames("Omega notes", ["Delta notes"])).toEqual(["Delta notes"]));
  it("matches edit distance 2", () => expect(similarNames("Gamme", ["Gamma"])).toEqual(["Gamma"]));
  it("keeps short names to edit distance 1", () => expect(similarNames("Beta", ["Delta"])).toEqual([]));
  it("excludes the exact name and unrelated ones", () => {
    expect(similarNames("Delta", ["Delta", "Omega"])).toEqual([]);
    expect(similarNames("", ["Delta"])).toEqual([]);
  });
  it("lists the closer match first", () => expect(similarNames("Alpha", ["Alpha fixing", "alpha"])).toEqual(["alpha", "Alpha fixing"]));
});
