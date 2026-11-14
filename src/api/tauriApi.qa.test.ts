import { beforeEach, describe, expect, it, vi } from "vitest";
import { makeDemoSnapshot, makeSeedSnapshot } from "./mockData";
import { makeSettings } from "./mockSettings";
import { parseWire, settingsSchema, snapshotSchema } from "./wire";

const invoke = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => undefined) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => undefined) }) }));

const { TauriApi } = await import("./tauriApi");

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
});

describe("TauriApi QA calls", () => {
  it("calls each command with its arguments", async () => {
    const api = new TauriApi();
    const head = "0123456789abcdef0123456789abcdef01234567";
    await api.setFloorQa("p-1", true);
    await api.recheckQa();
    await api.openReview("p-1", 12, head);
    expect(invoke.mock.calls).toEqual([
      ["set_floor_qa", { projectId: "p-1", enabled: true }],
      ["recheck_qa", undefined],
      ["open_review", { projectId: "p-1", number: 12, head }],
    ]);
  });
});

describe("wire QA shapes", () => {
  it("parses a snapshot with every status kind", () => {
    const snapshot = makeDemoSnapshot();
    const kinds = snapshot.qa.map((q) => q.status.kind).sort();
    expect(kinds).toEqual(["blocked", "off", "watching", "watching"]);
    expect(parseWire(snapshotSchema, "snapshot", snapshot)).toEqual(snapshot);
    const waiting = { ...snapshot, qa: [{ projectId: "p-shop", enabled: true, status: { kind: "waiting" }, run: null, reviews: [], stamp: null }] };
    expect(parseWire(snapshotSchema, "snapshot", waiting).qa[0]?.status.kind).toBe("waiting");
  });

  it("parses each review outcome", () => {
    const base = makeSeedSnapshot();
    const review = (outcome: unknown): unknown => ({ repo: "example-org/widgets", number: 1, title: "t", head: "h", at: "2026-11-12T10:00:00Z", outcome });
    const floor = (outcome: unknown): unknown => ({ ...base, qa: [{ projectId: "p-shop", enabled: true, status: { kind: "off" }, run: null, reviews: [review(outcome)], stamp: null }] });
    for (const outcome of [{ kind: "clean" }, { kind: "failed", reason: "no" }, { kind: "posted", verdict: "pass", findings: 0, reviewId: 1 }]) {
      expect(() => parseWire(snapshotSchema, "snapshot", floor(outcome))).not.toThrow();
    }
    expect(() => parseWire(snapshotSchema, "snapshot", floor({ kind: "posted", verdict: "maybe", findings: 0, reviewId: 1 }))).toThrow();
  });

  it("rejects a snapshot without qa", () => {
    const { qa: _qa, ...rest } = makeSeedSnapshot();
    expect(() => parseWire(snapshotSchema, "snapshot", rest)).toThrow(/unexpected snapshot at qa/);
  });

  it("parses settings with the qa group and rejects them without it", () => {
    const settings = makeSettings();
    expect(parseWire(settingsSchema, "settings", settings)).toEqual(settings);
    const { qa: _qa, ...rest } = settings;
    expect(() => parseWire(settingsSchema, "settings", rest)).toThrow(/unexpected settings at qa/);
  });
});
