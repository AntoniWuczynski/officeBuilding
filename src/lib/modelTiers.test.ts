import { describe, expect, it } from "vitest";
import { makeHireOptions } from "../api/mockHireOptions";
import { BUILDS, BUILD_LABEL, buildOf, TIER_FAMILIES, TIER_WORKERS, tierOf } from "./modelTiers";

describe("tierOf", () => {
  // Real ids: the Hire sheet's lists, the catalogue fixture, the settings aliases and Codex's models cache.
  const known = [
    ["kit", ["claude-haiku-5-5", "haiku", "gpt-6-luna", "gpt-5.6-luna"]],
    ["nell", ["claude-sonnet-5-5", "claude-sonnet-4-6", "sonnet[1m]", "gpt-5.6-terra", "gpt-6.1-terra", "Claude Sonnet 4.6"]],
    ["otto", ["claude-opus-5-5", "claude-opus-4-8", "opus", "gpt-6-sol", "gpt-5.6-sol", "gpt-6.1-sol", "gpt-6.2-sol", "Claude Opus 4.6"]],
    ["vera", ["claude-fable-5-1", "fable", "gpt-6-astra", "gpt-6.2-astra"]],
  ] as const;

  it.each(known)("puts %s's models on it", (tier, ids) => {
    for (const id of ids) expect(tierOf(id), id).toBe(tier);
  });

  it.each(["gpt-5.5", "gpt-reserve", "codex-auto-review", "Gemini 3.6 Flash", "GPT-OSS 120B", "opencode/big-pickle", "default", ""])("has no tier for %j", (id) => {
    expect(tierOf(id)).toBeNull();
  });

  it("has no tier for a missing model", () => {
    expect(tierOf(null)).toBeNull();
  });

  it("matches a family only as a whole word", () => {
    for (const id of ["solar-1", "opusculum", "terraform"]) expect(tierOf(id), id).toBeNull();
  });

  it("lands ids that do not exist yet on their tier with no code change", () => {
    expect(tierOf("claude-opus-6")).toBe("otto");
    expect(tierOf("gpt-7-sol")).toBe("otto");
    expect(tierOf("claude-sonnet-5-5-20270101")).toBe("nell");
  });

  it("gives every tier two families and no family two tiers", () => {
    const families = TIER_WORKERS.flatMap((t) => TIER_FAMILIES[t]);
    expect(families).toHaveLength(TIER_WORKERS.length * 2);
    expect(new Set(families).size).toBe(families.length);
  });
});

describe("buildOf", () => {
  it("names the six builds, and ultra is max", () => {
    for (const b of BUILDS) expect(buildOf(b)).toBe(b);
    expect(buildOf("ultra")).toBe("max");
  });

  it("forgives case and spaces", () => {
    expect(buildOf(" High ")).toBe("high");
  });

  it.each(["default", "thinking", "constructor", "toString", ""])("has no build for %j", (id) => {
    expect(buildOf(id)).toBeNull();
  });

  it("has no build for a missing effort", () => {
    expect(buildOf(null)).toBeNull();
  });

  it("labels every build", () => {
    expect(BUILDS.map((b) => BUILD_LABEL[b])).toEqual(["Minimal", "Low", "Medium", "High", "Extra high", "Max"]);
  });

  it("covers every effort of every tool's models, lean to buff for Claude Code and Codex", () => {
    for (const tool of makeHireOptions()) {
      for (const model of tool.models) {
        const builds = model.efforts.map((e) => buildOf(e.id));
        model.efforts.forEach((e, i) => {
          if (builds[i] === null) expect(["default", "thinking"], `${model.id} ${e.id}`).toContain(e.id);
        });
        if (tool.tool === "claude-code" || tool.tool === "codex") {
          const ranks = builds.map((b) => (b === null ? -1 : BUILDS.indexOf(b)));
          expect(ranks, model.id).toEqual([...ranks].sort((a, b) => a - b));
          expect(ranks, model.id).not.toContain(-1);
        }
      }
    }
  });
});
