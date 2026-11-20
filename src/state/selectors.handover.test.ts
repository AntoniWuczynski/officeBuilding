import { describe, expect, it } from "vitest";
import { makeSeedSnapshot } from "../api/mockData";
import { makeHandoverView } from "../api/mockHandover";
import { handoverFor } from "./selectors";

describe("handoverFor", () => {
  it("finds the state of one desk, and nothing for another", () => {
    const state = { kind: "underway", to: "codex" } as const;
    const snapshot = { ...makeSeedSnapshot(), handover: makeHandoverView([{ sessionId: "s-1", state }]) };
    expect(handoverFor(snapshot, "s-1")).toEqual(state);
    expect(handoverFor(snapshot, "s-2")).toBeUndefined();
  });
});

describe("the take-overs of the handover view", () => {
  it("start empty and ride along with the desks", () => {
    expect(makeHandoverView().takeOvers).toEqual([]);
    const snapshot = { ...makeSeedSnapshot(), handover: { ...makeHandoverView(), takeOvers: [{ id: "h-1", fromSession: "s-1", from: "claude-code", to: "codex", toSession: null, at: 1 }] as const } };
    expect(snapshot.handover.takeOvers).toHaveLength(1);
    expect(handoverFor(snapshot, "s-1")).toBeUndefined();
  });
});
