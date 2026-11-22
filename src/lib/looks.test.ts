import { describe, expect, it } from "vitest";
import { TIER_BUILDS } from "../scene/cast/parts";
import type { Helper } from "../types";
import { helperDrawn, hashString, isUltracode, lookFor, lookOf } from "./looks";
import type { Drawn } from "./looks";
import { relativeTime } from "./time";

describe("lookFor", () => {
  it("is stable for an id", () => {
    expect(lookFor("s-1")).toEqual(lookFor("s-1"));
  });
  it("varies across ids", () => {
    const cast = new Set(["s-1", "s-2", "s-3", "s-4", "s-5", "s-6", "s-7", "s-8"].map((id) => lookFor(id).name));
    expect(cast.size).toBeGreaterThan(2);
  });
  it("hashString is FNV-1a", () => {
    expect(hashString("")).toBe(0x811c9dc5);
    expect(hashString("a")).toBe(0xe40c292c);
  });
});

const FIRST_CAST = ["theo", "mara", "walt", "ayo", "dev", "fern"];
const desk = (model: string | null, effort: string | null, id = "s-1"): Drawn => ({ id, tool: "claude-code", model, effort });

describe("lookOf", () => {
  it("draws a model's tier worker in its effort's build, whatever the provider", () => {
    const sonnet = lookOf(desk("claude-sonnet-5-5", "high"));
    expect(sonnet.name).toBe("nell-high");
    expect(lookOf(desk("gpt-5.6-terra", "high", "s-2"))).toBe(sonnet);
  });

  it.each([
    ["claude-haiku-5-5", "gpt-5.6-luna", "kit"],
    ["claude-opus-5-5", "gpt-6-sol", "otto"],
    ["claude-fable-5-1", "gpt-6-astra", "vera"],
  ])("puts %s and %s on %s", (anthropic, openai, worker) => {
    expect(lookOf(desk(anthropic, "low")).name).toBe(`${worker}-low`);
    expect(lookOf(desk(openai, "low")).name).toBe(`${worker}-low`);
  });

  it("gives a known model with no build of its own the medium build", () => {
    for (const effort of [null, "default", "thinking", "turbo"]) expect(lookOf(desk("claude-opus-5-5", effort)).name).toBe("otto-medium");
  });

  it("gives an unknown or missing model the id's own cast member, whatever the effort", () => {
    for (const model of [null, "gpt-5.5", ""]) {
      for (const effort of [null, "low", "max"]) {
        const look = lookOf(desk(model, effort, "s-7"));
        expect(look).toBe(lookFor("s-7"));
        expect(FIRST_CAST).toContain(look.name);
      }
    }
  });

  it("is stable for the same session", () => {
    expect(lookOf(desk("claude-opus-5-5", "xhigh"))).toBe(lookOf(desk("claude-opus-5-5", "xhigh")));
  });

  it("takes a tier look's face and chair from the medium sheet, the rest from the build, and the medium as its frame", () => {
    const look = lookOf(desk("haiku", "max"));
    expect(look.parts.head).toBe(TIER_BUILDS.kit.medium.head);
    expect(look.parts["head-closed"]).toBe(TIER_BUILDS.kit.medium["head-closed"]);
    expect(look.parts.chair).toBe(TIER_BUILDS.kit.medium.chair);
    expect(look.parts.torso).toBe(TIER_BUILDS.kit.max.torso);
    expect(look.frame).toBe(TIER_BUILDS.kit.medium);
  });

  it("tints a look with ultracode on and keeps its parts", () => {
    const plain = lookOf(desk("claude-opus-5-5", "high"));
    const tinted = lookOf(desk("claude-opus-5-5", "high"), true);
    expect(plain.tint).toBeUndefined();
    expect(tinted).toMatchObject({ name: plain.name, tint: true });
    expect(tinted.parts).toBe(plain.parts);
    expect(lookOf(desk(null, null), true)).toMatchObject({ name: lookFor("s-1").name, tint: true });
  });
});

describe("helperDrawn", () => {
  const agent: Drawn = { id: "s-1", tool: "codex", model: "gpt-6-astra", effort: "xhigh" };
  const helper = (model: string | null, effort: string | null): Helper => ({ id: "Ada", state: "running", worktree: false, branch: null, model, effort });

  it("draws a helper by its own model and effort, in its agent's tool, under its own id", () => {
    expect(helperDrawn(agent, helper("gpt-5.6-luna", "low"))).toEqual({ id: "Ada", tool: "codex", model: "gpt-5.6-luna", effort: "low" });
  });

  it("falls back to the agent field by field", () => {
    expect(helperDrawn(agent, helper("gpt-5.6-luna", null))).toEqual({ id: "Ada", tool: "codex", model: "gpt-5.6-luna", effort: "xhigh" });
    expect(helperDrawn(agent, helper(null, "low"))).toEqual({ id: "Ada", tool: "codex", model: "gpt-6-astra", effort: "low" });
    expect(helperDrawn(agent, helper(null, null))).toEqual({ id: "Ada", tool: "codex", model: "gpt-6-astra", effort: "xhigh" });
  });
});

describe("isUltracode", () => {
  const set = new Set(["s-1", "s-2"]);
  it("is true only for a Claude Code session in the set", () => {
    expect(isUltracode(set, { id: "s-1", tool: "claude-code" })).toBe(true);
    expect(isUltracode(set, { id: "s-2", tool: "codex" })).toBe(false);
    expect(isUltracode(set, { id: "s-3", tool: "claude-code" })).toBe(false);
    expect(isUltracode(new Set(), { id: "s-1", tool: "claude-code" })).toBe(false);
  });
});

describe("relativeTime", () => {
  const now = new Date("2026-09-29T12:00:00Z");
  it("reads in plain units", () => {
    expect(relativeTime("2026-09-29T11:59:30Z", now)).toBe("under a minute ago");
    expect(relativeTime("2026-09-29T11:56:00Z", now)).toBe("4 min ago");
    expect(relativeTime("2026-09-29T09:00:00Z", now)).toBe("3 h ago");
    expect(relativeTime("2026-09-28T11:00:00Z", now)).toBe("1 day ago");
    expect(relativeTime("2026-09-26T11:00:00Z", now)).toBe("3 days ago");
  });
  it("never goes negative and admits bad input", () => {
    expect(relativeTime("2026-09-30T00:00:00Z", now)).toBe("under a minute ago");
    expect(relativeTime("not a date", now)).toBe("at an unknown time");
  });
});
