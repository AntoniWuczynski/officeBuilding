import { describe, expect, it } from "vitest";
import type { ProjectRule, Project, Session } from "../types";
import { nameKey, placeSession, repoFolder } from "./projectGuess";

const rule = (id: string, p: Partial<ProjectRule> = {}): ProjectRule => ({ id, name: id, prefixes: [], contains: [], repos: [], ...p });
const sess = (name: string | null, source: Session["sessionNameSource"] = "user"): Session =>
  ({ id: "s", sessionName: name, sessionNameSource: name === null ? null : source }) as Session;
const none = new Map<string, number>();

describe("nameKey", () => {
  it("strips a trailing -x then -digits, lower-cased", () => {
    expect(nameKey("alpha-fix-2-X")).toBe("alpha-fix");
    expect(nameKey("Gamma-3-x")).toBe("gamma");
    expect(nameKey("gamma-wave3")).toBe("gamma-wave3");
    expect(nameKey("x")).toBe("x");
  });
});

describe("repoFolder", () => {
  it("uses path, so a renamed island keeps its folder", () => {
    expect(repoFolder({ name: "Renamed island", path: "/x/repo-one" } as Project)).toBe("repo-one");
    expect(repoFolder({ name: "n", path: "/x/repo-one/" } as Project)).toBe("repo-one");
  });
});

describe("placeSession rules", () => {
  const rules = [rule("gamma", { prefixes: ["gamma-"] }), rule("delta", { prefixes: ["del"] })];
  it("matches on a hyphen boundary", () => {
    expect(placeSession(sess("gamma-3-X"), "f", rules, none, undefined)).toEqual({ kind: "rule", projectId: "gamma" });
    expect(placeSession(sess("gamma"), "f", rules, none, undefined)).toEqual({ kind: "rule", projectId: "gamma" });
    expect(placeSession(sess("gammaray-1"), "f", rules, none, undefined)).toEqual({ kind: "unplaced" });
    expect(placeSession(sess("delta-notes"), "f", rules, none, undefined)).toEqual({ kind: "rule", projectId: "delta" });
  });
  it("longest prefix wins, equal length goes to the earlier rule", () => {
    const r = [rule("short", { prefixes: ["al"] }), rule("long", { prefixes: ["alpha-"] })];
    expect(placeSession(sess("alpha-fix"), "f", r, none, undefined)).toEqual({ kind: "rule", projectId: "long" });
    const tie = [rule("one", { prefixes: ["alpha-"] }), rule("two", { prefixes: ["ALPHA-"] })];
    expect(placeSession(sess("alpha-fix"), "f", tie, none, undefined)).toEqual({ kind: "rule", projectId: "one" });
  });
  it("contains places, and a prefix rule beats a contains rule", () => {
    const k = rule("kappa", { contains: ["kappa"] });
    expect(placeSession(sess("for-kappa"), "f", [k], none, undefined)).toEqual({ kind: "rule", projectId: "kappa" });
    const beta = rule("beta", { prefixes: ["for-"] });
    expect(placeSession(sess("for-kappa"), "f", [k, beta], none, undefined)).toEqual({ kind: "rule", projectId: "beta" });
  });
});

describe("placeSession guesses", () => {
  it("guesses the repo folder at a hyphen boundary", () => {
    expect(placeSession(sess("repo-one-lane"), "repo-one", [], none, undefined)).toEqual({ kind: "guess", projectId: "repo-one" });
    expect(placeSession(sess("apple-x"), "app", [], none, undefined)).toEqual({ kind: "unplaced" });
  });
  it("guesses a shared first word, leaves a lone name unplaced", () => {
    const live = new Map([["beta", 2]]);
    expect(placeSession(sess("beta-web"), "f", [], live, undefined)).toEqual({ kind: "guess", projectId: "beta" });
    expect(placeSession(sess("beta-api"), "f", [], live, undefined)).toEqual({ kind: "guess", projectId: "beta" });
    expect(placeSession(sess("solo-thing"), "f", [], none, undefined)).toEqual({ kind: "unplaced" });
  });
  it("keeps a sticky guess once the partner is gone, but rules still win", () => {
    const prev = { kind: "guess", projectId: "beta" } as const;
    expect(placeSession(sess("beta-web"), "f", [], none, prev)).toEqual(prev);
    expect(placeSession(sess("beta-web"), "f", [rule("b", { prefixes: ["beta-"] })], none, prev)).toEqual({ kind: "rule", projectId: "b" });
  });
  it("unnamed sessions follow their island", () => {
    expect(placeSession(sess(null), "f", [rule("a", { contains: ["a"] })], none, undefined)).toEqual({ kind: "island" });
  });
});

describe("placeSession name source", () => {
  const live = new Map([["fix", 2]]);
  it("auto names get no two-session guess and are never unplaced", () => {
    expect(placeSession(sess("fix-login-a", "auto"), "f", [], live, undefined)).toEqual({ kind: "island" });
    expect(placeSession(sess("fix-tests-b", "auto"), "f", [], live, undefined)).toEqual({ kind: "island" });
  });
  it("auto names still use rules and the folder guess", () => {
    expect(placeSession(sess("repo-one-lane", "auto"), "repo-one", [], none, undefined)).toEqual({ kind: "guess", projectId: "repo-one" });
    expect(placeSession(sess("fix-a", "auto"), "f", [rule("r", { prefixes: ["fix-"] })], none, undefined)).toEqual({ kind: "rule", projectId: "r" });
  });
  it("user and peer names get the two-session guess and are asked", () => {
    expect(placeSession(sess("fix-login-a", "user"), "f", [], live, undefined)).toEqual({ kind: "guess", projectId: "fix" });
    expect(placeSession(sess("fix-tests-b", "peer"), "f", [], live, undefined)).toEqual({ kind: "guess", projectId: "fix" });
    expect(placeSession(sess("fix-login-a", "user"), "f", [], none, undefined)).toEqual({ kind: "unplaced" });
  });
});
