import { describe, expect, it } from "vitest";
import { MockHandover, makeHandoverSettings, makeHandoverView } from "./mockHandover";

describe("MockHandover", () => {
  it("records each desk in order", async () => {
    const api = new MockHandover();
    await api.handOverDesk("a");
    await api.handOverDesk("b");
    expect(api.handed).toEqual(["a", "b"]);
  });
  it("fails the next call only, and records nothing for it", async () => {
    const api = new MockHandover();
    api.failNext("no luck");
    await expect(api.handOverDesk("a")).rejects.toThrow("no luck");
    await api.handOverDesk("b");
    expect(api.handed).toEqual(["b"]);
  });
  it("makes an empty group and view", () => {
    expect(makeHandoverSettings()).toEqual({ automatic: false, claudeToCodex: null, codexToClaude: null });
    expect(makeHandoverView()).toEqual({ usedUp: [], desks: [] });
    const desk = { sessionId: "s", state: { kind: "underway", to: "codex" } } as const;
    expect(makeHandoverView([desk]).desks).toEqual([desk]);
  });
});
