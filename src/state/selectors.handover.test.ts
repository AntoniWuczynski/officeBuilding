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
