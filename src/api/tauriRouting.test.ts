import { beforeEach, describe, expect, it, vi } from "vitest";
import { addConnection, addContext } from "../lib/routing";
import type { RoutingSettings } from "../types";
import { nth } from "../test/nth";
import { makeRoutingSettings } from "./mockRouting";
import { routingSettingsSchema } from "./routingWire";

const invoke = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => undefined) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => undefined) }) }));

const { TauriRouting } = await import("./tauriRouting");

const VIEW = { enabled: true, context: "ctx-1", contexts: [{ id: "ctx-1", name: "Work" }] };

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
});

describe("TauriRouting", () => {
  it("sends each command with its arguments and parses the reply", async () => {
    const api = new TauriRouting();
    invoke.mockResolvedValueOnce({ kind: "saved" });
    await expect(api.keyStatus("con-1")).resolves.toEqual({ kind: "saved" });
    invoke.mockResolvedValueOnce({ kind: "unavailable", reason: "locked" });
    await expect(api.setKey("con-1", "sk-demo")).resolves.toEqual({ kind: "unavailable", reason: "locked" });
    invoke.mockResolvedValueOnce(VIEW);
    await expect(api.floorRouting("p-1")).resolves.toEqual(VIEW);
    invoke.mockResolvedValueOnce(VIEW);
    await expect(api.setFloorContext("p-1", null)).resolves.toEqual(VIEW);
    expect(invoke.mock.calls).toEqual([
      ["routing_key_status", { connectionId: "con-1" }],
      ["set_routing_key", { connectionId: "con-1", key: "sk-demo" }],
      ["floor_routing", { projectId: "p-1" }],
      ["set_floor_context", { projectId: "p-1", contextId: null }],
    ]);
  });

  it("rejects a reply of the wrong shape", async () => {
    const api = new TauriRouting();
    invoke.mockResolvedValueOnce({ kind: "unavailable" });
    await expect(api.keyStatus("con-1")).rejects.toThrow(/unexpected key status/);
    invoke.mockResolvedValueOnce({ kind: "other" });
    await expect(api.setKey("con-1", null)).rejects.toThrow(/unexpected key status/);
    invoke.mockResolvedValueOnce({ enabled: true });
    await expect(api.floorRouting("p-1")).rejects.toThrow(/unexpected floor routing/);
    invoke.mockResolvedValueOnce(null);
    await expect(api.setFloorContext("p-1", "ctx-1")).rejects.toThrow(/unexpected floor routing/);
  });

  it("passes a backend error through as an Error", async () => {
    const api = new TauriRouting();
    invoke.mockRejectedValueOnce("that connection is no longer in Settings");
    await expect(api.keyStatus("con-1")).rejects.toThrow("that connection is no longer in Settings");
  });
});

describe("routingSettingsSchema", () => {
  const full = (): RoutingSettings => {
    const s = addConnection(addContext(makeRoutingSettings(), "ctx-1", "Work"), "con-1", "ctx-1", "openrouter");
    return { ...s, tiers: { ...s.tiers, haiku: ["m"] }, connections: s.connections.map((c) => ({ ...c, targets: { ...c.targets, haiku: { model: "m", efforts: { xhigh: "high" } } } })) };
  };

  it("accepts the default and a full example", () => {
    expect(routingSettingsSchema.parse(makeRoutingSettings())).toEqual(makeRoutingSettings());
    expect(routingSettingsSchema.parse(full())).toEqual(full());
  });

  it("rejects an unknown kind, a missing tier key and a target with no efforts", () => {
    const base = full();
    const conn = nth(base.connections, 0);
    expect(routingSettingsSchema.safeParse({ ...base, connections: [{ ...conn, kind: "gemini" }] }).success).toBe(false);
    const { haiku: _haiku, ...tiers } = base.tiers;
    expect(routingSettingsSchema.safeParse({ ...base, tiers }).success).toBe(false);
    expect(routingSettingsSchema.safeParse({ ...base, connections: [{ ...conn, targets: { ...conn.targets, haiku: { model: "m" } } }] }).success).toBe(false);
  });
});
