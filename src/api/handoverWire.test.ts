import { describe, expect, it } from "vitest";
import { handoverSettingsSchema, handoverViewSchema } from "./handoverWire";
import { makeSeedSnapshot } from "./mockData";
import { makeHandoverSettings } from "./mockHandover";
import { makeSettings } from "./mockSettings";
import { parseWire, settingsSchema, snapshotSchema } from "./wire";

// The JSON shapes of section 3 of the handover spec, as the backend sends them.
const STATES: readonly unknown[] = [
  { kind: "offered", to: "codex", why: { kind: "automatic-off" }, work: { kind: "clear" }, window: "five-hour", resetAt: 1700000000 },
  { kind: "offered", to: "codex", why: { kind: "failed", reason: "no" }, work: { kind: "running", tasks: [{ kind: "workflow", name: "w" }, { kind: "shell", name: null }, { kind: "goal", name: "g" }] }, window: null, resetAt: null },
  { kind: "offered", to: "codex", why: { kind: "work-unknown" }, work: { kind: "unknown" }, window: "seven-day", resetAt: 1 },
  { kind: "blocked", to: "codex", why: "both-used-up", window: null, resetAt: null, otherResetAt: 5 },
  { kind: "underway", to: "codex" },
  { kind: "arrived", from: "claude-code", at: 1700000000, window: "five-hour", resetAt: null },
  { kind: "left", to: "codex", at: 1700000000 },
  { kind: "failed", to: "codex", at: 1700000000, reason: "x" },
];

describe("handoverViewSchema", () => {
  it("accepts a payload of every state", () => {
    const view = { usedUp: [{ tool: "claude-code", window: "five-hour", resetAt: null, seenAt: 3 }], desks: STATES.map((state, i) => ({ sessionId: `s${i}`, state })) };
    expect(parseWire(handoverViewSchema, "handover", view)).toEqual(view);
    expect(parseWire(handoverViewSchema, "handover", { usedUp: [], desks: [] })).toEqual({ usedUp: [], desks: [] });
  });
  it("refuses an unknown task kind", () => {
    const state = { kind: "offered", to: "codex", why: { kind: "work-running" }, work: { kind: "running", tasks: [{ kind: "chore", name: null }] }, window: null, resetAt: null };
    expect(() => parseWire(handoverViewSchema, "handover", { usedUp: [], desks: [{ sessionId: "s", state }] })).toThrow(/kind/);
  });
  it("refuses an unknown kind and a missing field", () => {
    expect(() => parseWire(handoverViewSchema, "handover", { usedUp: [], desks: [{ sessionId: "s", state: { kind: "gone", to: "codex" } }] })).toThrow(/unexpected handover/);
    expect(() => parseWire(handoverViewSchema, "handover", { usedUp: [], desks: [{ sessionId: "s", state: { kind: "left", to: "codex" } }] })).toThrow(/at/);
    expect(() => parseWire(handoverViewSchema, "handover", { desks: [] })).toThrow(/usedUp/);
  });
});

describe("handoverSettingsSchema", () => {
  it("accepts both start choices and null", () => {
    expect(parseWire(handoverSettingsSchema, "settings", makeHandoverSettings())).toEqual(makeHandoverSettings());
    const chosen = { automatic: true, claudeToCodex: { kind: "chosen", model: "m", effort: "high" }, codexToClaude: null };
    expect(parseWire(handoverSettingsSchema, "settings", chosen)).toEqual(chosen);
    const dflt = { automatic: false, claudeToCodex: { kind: "tool-default" }, codexToClaude: null };
    expect(parseWire(handoverSettingsSchema, "settings", dflt)).toEqual(dflt);
  });
  it("refuses an unknown start kind", () => {
    expect(() => parseWire(handoverSettingsSchema, "settings", { automatic: false, claudeToCodex: { kind: "other" }, codexToClaude: null })).toThrow(/claudeToCodex/);
  });
});

describe("the snapshot and the settings", () => {
  it("refuse a payload without handover", () => {
    const { handover: _view, ...noView } = makeSeedSnapshot();
    expect(() => parseWire(snapshotSchema, "snapshot", noView)).toThrow(/unexpected snapshot at handover/);
    const { handover: _group, ...noGroup } = makeSettings();
    expect(() => parseWire(settingsSchema, "settings", noGroup)).toThrow(/unexpected settings at handover/);
  });
});
