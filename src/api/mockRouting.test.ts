import { describe, expect, it } from "vitest";
import { addConnection, addContext } from "../lib/routing";
import type { Settings } from "../types";
import { MockRouting, makeDeskRoute, makeRoutingSettings } from "./mockRouting";
import { makeSettings } from "./mockSettings";

function setup(): { routing: MockRouting; settings: { current: Settings } } {
  let routing = addContext(makeRoutingSettings(), "ctx-1", "Work");
  routing = addConnection(routing, "con-sub", "ctx-1", "claude-subscription");
  routing = addConnection(routing, "con-key", "ctx-1", "anthropic-api-key");
  const settings = { current: { ...makeSettings(), routing } };
  return { routing: new MockRouting(() => Promise.resolve(settings.current)), settings };
}

describe("MockRouting keys", () => {
  it("is missing, then saved, then missing again", async () => {
    const { routing } = setup();
    await expect(routing.keyStatus("con-key")).resolves.toEqual({ kind: "missing" });
    await expect(routing.setKey("con-key", " sk-demo-123 ")).resolves.toEqual({ kind: "saved" });
    await expect(routing.keyStatus("con-key")).resolves.toEqual({ kind: "saved" });
    await expect(routing.setKey("con-key", null)).resolves.toEqual({ kind: "missing" });
    await expect(routing.keyStatus("con-key")).resolves.toEqual({ kind: "missing" });
  });

  it("refuses with the backend's words", async () => {
    const { routing } = setup();
    await expect(routing.keyStatus("con-zz")).rejects.toThrow("that connection is no longer in Settings");
    await expect(routing.setKey("con-sub", "sk-demo")).rejects.toThrow("that kind of connection takes no API key");
    await expect(routing.keyStatus("con-sub")).rejects.toThrow("that kind of connection takes no API key");
    const bad = ["", "   ", "x".repeat(501), "has space", "naïve"];
    for (const key of bad) await expect(routing.setKey("con-key", key), JSON.stringify(key)).rejects.toThrow("that does not look like an API key");
    await expect(routing.setKey("con-key", "x".repeat(500))).resolves.toEqual({ kind: "saved" });
  });

  it("holds the key text nowhere", async () => {
    const { routing } = setup();
    await routing.setKey("con-key", "sk-secret-text");
    expect(routing.held()).toEqual(["con-key"]);
  });
});

describe("MockRouting floors", () => {
  it("shows no context, one chosen, and none again", async () => {
    const { routing } = setup();
    const contexts = [{ id: "ctx-1", name: "Work" }];
    await expect(routing.floorRouting("p-1")).resolves.toEqual({ enabled: false, context: null, contexts });
    await expect(routing.setFloorContext("p-1", "ctx-1")).resolves.toEqual({ enabled: false, context: "ctx-1", contexts });
    await expect(routing.floorRouting("p-1")).resolves.toMatchObject({ context: "ctx-1" });
    await expect(routing.floorRouting("p-2")).resolves.toMatchObject({ context: null });
    await expect(routing.setFloorContext("p-1", null)).resolves.toMatchObject({ context: null });
  });

  it("refuses an unknown context and hides one that has left Settings", async () => {
    const { routing, settings } = setup();
    await expect(routing.setFloorContext("p-1", "ctx-9")).rejects.toThrow("that account context is no longer in Settings");
    await routing.setFloorContext("p-1", "ctx-1");
    settings.current = { ...settings.current, routing: { ...settings.current.routing, contexts: [] } };
    await expect(routing.floorRouting("p-1")).resolves.toEqual({ enabled: false, context: null, contexts: [] });
  });
});

const SCOPE = { bucket: { kind: "whole" }, window: null, observedAt: 100, resetAt: null, checking: false, retryArmed: false } as const;

function routed(): MockRouting {
  const { routing } = setup();
  const route = makeDeskRoute();
  const [sub, api] = route.status.entries;
  if (sub === undefined || api === undefined) throw new Error("makeDeskRoute needs two entries");
  routing.setRoute("s-1", {
    ...route,
    status: { ...route.status, blockedSince: 50, entries: [{ ...sub, exhausted: [SCOPE, { ...SCOPE, checking: true }] }, api, { ...api, connection: "bad", state: { kind: "unavailable", reason: { kind: "no-credential" } } }] },
  });
  return routing;
}

describe("MockRouting routes", () => {
  it("has none until one is set, and none again after null", async () => {
    const { routing } = setup();
    await expect(routing.deskRoute("s-1")).resolves.toBeNull();
    routing.setRoute("s-1", makeDeskRoute());
    await expect(routing.deskRoute("s-1")).resolves.toEqual(makeDeskRoute());
    routing.setRoute("s-1", null);
    await expect(routing.deskRoute("s-1")).resolves.toBeNull();
  });

  it("arms the scopes that are not being checked on a retry", async () => {
    const routing = routed();
    const view = await routing.retryRoute("s-1", "sub");
    expect(view.status.entries[0]?.exhausted.map((s) => s.retryArmed)).toEqual([true, false]);
  });

  it("refuses with the backend's words", async () => {
    const routing = routed();
    await expect(routing.retryRoute("s-9", "sub")).rejects.toThrow("that desk is not routed");
    await expect(routing.switchRoute("s-9", "sub")).rejects.toThrow("that desk is not routed");
    await expect(routing.retryRoute("s-1", "zz")).rejects.toThrow("that connection is not in this desk's order");
    await expect(routing.retryRoute("s-1", "api")).rejects.toThrow("that connection has no used-up allowance on record, so there is nothing to retry");
    await expect(routing.switchRoute("s-1", "bad")).rejects.toThrow("Anthropic key cannot be switched to, because no API key is saved for it");
  });

  it("moves the active entry on a switch, records it and clears the block", async () => {
    const routing = routed();
    const view = await routing.switchRoute("s-1", "api");
    expect(view.status.active).toBe("api");
    expect(view.status.generation).toBe(2);
    expect(view.status.blockedSince).toBeNull();
    expect(view.status.lastSwitch).toMatchObject({ from: "sub", to: "api", reason: "manual" });
    expect(view.status.entries.map((e) => e.state.kind)).toEqual(["ready", "active", "unavailable"]);
    const back = await routing.switchRoute("s-1", "sub");
    expect(back.status.entries[0]?.exhausted.map((x) => x.retryArmed)).toEqual([true, false]);
    await expect(routing.deskRoute("s-1")).resolves.toEqual(back);
  });
});
