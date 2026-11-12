import { beforeEach, describe, expect, it, vi } from "vitest";
import { makeSeedSnapshot } from "./mockData";
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

describe("TauriApi manager calls", () => {
  it("calls each command with its arguments", async () => {
    const api = new TauriApi();
    await api.hireManager("p-1");
    await api.setManagerPaused("p-1", true);
    await api.setManagerHiring("p-1", false);
    await api.withdrawManagerNote("p-1", "n-1");
    expect(invoke.mock.calls).toEqual([
      ["hire_manager", { projectId: "p-1" }],
      ["set_manager_paused", { projectId: "p-1", paused: true }],
      ["set_manager_hiring", { projectId: "p-1", allowed: false }],
      ["withdraw_manager_note", { projectId: "p-1", noteId: "n-1" }],
    ]);
  });
});

describe("wire manager shapes", () => {
  it("parses a snapshot with a manager entry of each status", () => {
    const snapshot = makeSeedSnapshot();
    expect(snapshot.managers.map((m) => m.status.kind)).toEqual(["hired", "absent", "hired", "absent"]);
    expect(parseWire(snapshotSchema, "snapshot", snapshot)).toEqual(snapshot);
  });

  it("parses each log event kind", () => {
    const base = makeSeedSnapshot();
    const [first] = base.managers;
    if (first === undefined) throw new Error("no manager");
    const events = [
      { kind: "note", noteId: "n", toSessionId: "s", toTitle: "t", text: "x", state: "failed" },
      { kind: "hire", tool: "codex", task: "x" },
      { kind: "escalation", title: "x" },
      { kind: "refused", tool: "hire_desk", reason: "x" },
    ];
    const log = events.map((event, i) => ({ id: `l-${i}`, at: "2026-11-12T10:00:00Z", event }));
    const parsed = parseWire(snapshotSchema, "snapshot", { ...base, managers: [{ ...first, log }] });
    expect(parsed.managers[0]?.log.map((e) => e.event.kind)).toEqual(["note", "hire", "escalation", "refused"]);
  });

  it("rejects an unknown hold or note state", () => {
    const base = makeSeedSnapshot();
    const [first] = base.managers;
    if (first === undefined) throw new Error("no manager");
    const note = { id: "n", toSessionId: "s", text: "x", queuedAt: "2026-11-12T10:00:00Z", hold: "sulking" };
    expect(() => parseWire(snapshotSchema, "snapshot", { ...base, managers: [{ ...first, queue: [note] }] })).toThrow(/unexpected snapshot/);
    const entry = { id: "l", at: "2026-11-12T10:00:00Z", event: { kind: "note", noteId: "n", toSessionId: "s", toTitle: "t", text: "x", state: "lost" } };
    expect(() => parseWire(snapshotSchema, "snapshot", { ...base, managers: [{ ...first, log: [entry] }] })).toThrow(/unexpected snapshot/);
  });

  it("parses answerVia store and rejects a snapshot without managers", () => {
    const base = makeSeedSnapshot();
    const [q] = base.questions;
    if (q === undefined) throw new Error("no question");
    expect(parseWire(snapshotSchema, "snapshot", { ...base, questions: [{ ...q, answerVia: "store" }] }).questions[0]?.answerVia).toBe("store");
    const { managers: _managers, ...rest } = base;
    expect(() => parseWire(snapshotSchema, "snapshot", rest)).toThrow(/unexpected snapshot at managers/);
  });

  it("parses settings with the manager group and rejects them without it", () => {
    const settings = makeSettings();
    expect(parseWire(settingsSchema, "settings", settings)).toEqual(settings);
    const { manager: _manager, ...rest } = settings;
    expect(() => parseWire(settingsSchema, "settings", rest)).toThrow(/unexpected settings at manager/);
  });
});
