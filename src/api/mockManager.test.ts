import { describe, expect, it } from "vitest";
import { MockApi } from "./mockApi";
import type { ManagerFloor } from "../types";
import { makeSeedSnapshot } from "./mockData";
import { makeAbsentManager, makeManagerSettings, makeSeedManagers, managerPatchError, hasQueuedNote, withManagerHired, withManagerSwitch, withNoteWithdrawn } from "./mockManager";

describe("mockManager", () => {
  it("seeds one entry per seeded floor", () => {
    const managers = makeSeedManagers();
    expect(managers.map((m) => m.projectId)).toEqual(makeSeedSnapshot().projects.map((p) => p.id));
    const [shop, office, arcade, notes] = managers;
    expect(shop?.status).toMatchObject({ kind: "hired", sessionId: null });
    expect(shop?.mayHire).toBe(true);
    expect(shop?.managed).toEqual(["s-1"]);
    expect(shop?.queue).toHaveLength(1);
    expect(shop?.queue[0]?.hold).toBe("busy");
    expect(shop?.log.map((e) => (e.event.kind === "note" ? e.event.state : e.event.kind))).toEqual(["refused", "queued", "delivered"]);
    expect(office?.status.kind).toBe("absent");
    expect(arcade?.status.kind).toBe("hired");
    expect(arcade?.paused).toBe(true);
    expect(arcade?.queue).toEqual([]);
    expect(notes?.status.kind).toBe("absent");
  });

  it("has the default settings", () => {
    expect(makeManagerSettings()).toEqual({ tool: "claude-code", model: null, effort: null, maxDesks: 3, spendCeilingUsd: 20 });
  });

  it("words the refusals", () => {
    const ok = makeManagerSettings();
    expect(managerPatchError(ok)).toBeNull();
    expect(managerPatchError({ ...ok, maxDesks: 0 })).toBe("desks at once must be between 1 and 10");
    expect(managerPatchError({ ...ok, maxDesks: 11 })).toBe("desks at once must be between 1 and 10");
    expect(managerPatchError({ ...ok, maxDesks: 2.5 })).toBe("desks at once must be between 1 and 10");
    expect(managerPatchError({ ...ok, spendCeilingUsd: 1001 })).toBe("the spend ceiling must be between 1 and 1000 dollars");
    expect(managerPatchError({ ...ok, spendCeilingUsd: 1000 })).toBeNull();
  });

  it("hires a manager, adding an entry for an unknown floor, without touching the input", () => {
    const list = makeSeedManagers();
    const hired = withManagerHired(list, "p-office", "2026-11-12T10:00:00Z");
    expect(hired.find((m) => m.projectId === "p-office")?.status).toEqual({ kind: "hired", sessionId: null, startedAt: "2026-11-12T10:00:00Z", lastCallAt: null });
    expect(list.find((m) => m.projectId === "p-office")?.status.kind).toBe("absent");
    expect(hired.find((m) => m.projectId === "p-shop")).toBe(list[0]);
    const added = withManagerHired(list, "p-new", "2026-11-12T10:00:00Z");
    expect(added).toHaveLength(list.length + 1);
    expect(added[added.length - 1]?.status.kind).toBe("hired");
  });

  it("sets a switch, adding an entry for an unknown floor", () => {
    const list = makeSeedManagers();
    expect(withManagerSwitch(list, "p-office", "paused", true).find((m) => m.projectId === "p-office")?.paused).toBe(true);
    expect(withManagerSwitch(list, "p-shop", "mayHire", false).find((m) => m.projectId === "p-shop")?.mayHire).toBe(false);
    const added = withManagerSwitch(list, "p-new", "mayHire", true);
    expect(added[added.length - 1]).toEqual({ ...makeAbsentManager("p-new"), mayHire: true });
  });

  it("withdraws a waiting note and marks its log entry", () => {
    const list = makeSeedManagers();
    expect(hasQueuedNote(list, "p-shop", "n-2")).toBe(true);
    expect(hasQueuedNote(list, "p-shop", "n-1")).toBe(false);
    expect(hasQueuedNote(list, "p-office", "n-2")).toBe(false);
    const next = withNoteWithdrawn(list, "p-shop", "n-2");
    const shop = next.find((m) => m.projectId === "p-shop");
    expect(shop?.queue).toEqual([]);
    expect(shop?.log.map((e) => (e.event.kind === "note" ? e.event.state : e.event.kind))).toEqual(["refused", "withdrawn", "delivered"]);
    expect(next.find((m) => m.projectId === "p-office")).toBe(list[1]);
  });
});

async function managers(api: MockApi): Promise<readonly ManagerFloor[]> {
  return (await api.getSnapshot()).managers;
}

describe("MockApi manager calls", () => {
  it("hires a manager and refuses the setup floor, an unknown floor and a second hire", async () => {
    const api = new MockApi();
    await api.hireManager("p-office");
    expect((await managers(api)).find((m) => m.projectId === "p-office")?.status).toMatchObject({ kind: "hired", sessionId: null, lastCallAt: null });
    await expect(api.hireManager("p-office")).rejects.toThrow("this floor already has a manager");
    await expect(api.hireManager("p-setup")).rejects.toThrow("the plant room has no manager");
    await expect(api.hireManager("p-nowhere")).rejects.toThrow("p-nowhere is not a floor of this building");
  });

  it("sets the pause and the hiring switch, refusing an unknown floor", async () => {
    const api = new MockApi();
    await api.setManagerPaused("p-shop", true);
    await api.setManagerHiring("p-shop", false);
    const shop = (await managers(api)).find((m) => m.projectId === "p-shop");
    expect(shop).toMatchObject({ paused: true, mayHire: false });
    await expect(api.setManagerPaused("p-nowhere", true)).rejects.toThrow("is not a floor of this building");
    await expect(api.setManagerHiring("p-nowhere", true)).rejects.toThrow("is not a floor of this building");
  });

  it("withdraws a waiting note once", async () => {
    const api = new MockApi();
    await api.withdrawManagerNote("p-shop", "n-2");
    expect((await managers(api)).find((m) => m.projectId === "p-shop")?.queue).toEqual([]);
    await expect(api.withdrawManagerNote("p-shop", "n-2")).rejects.toThrow("that note is no longer waiting");
  });

  it("refuses a manager patch with the backend's words and keeps a good one", async () => {
    const api = new MockApi();
    await expect(api.updateSettings({ manager: { ...makeManagerSettings(), maxDesks: 11 } })).rejects.toThrow("desks at once must be between 1 and 10");
    const manager = { ...makeManagerSettings(), maxDesks: 5 };
    await expect(api.updateSettings({ manager })).resolves.toMatchObject({ manager });
  });

  it("keeps managers in step with the floors", async () => {
    const api = new MockApi();
    const floor = await api.addFloor("/work/new-floor");
    expect((await managers(api)).find((m) => m.projectId === floor.id)?.status.kind).toBe("absent");
    await api.demolishFloor(floor.id);
    await api.demolishFloor("p-shop");
    expect((await managers(api)).map((m) => m.projectId)).toEqual(["p-office", "p-arcade", "p-notes"]);
  });
});
