import { describe, expect, it } from "vitest";
import { addConnection, addContext } from "../lib/routing";
import type { Settings } from "../types";
import { MockRouting, makeRoutingSettings } from "./mockRouting";
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
