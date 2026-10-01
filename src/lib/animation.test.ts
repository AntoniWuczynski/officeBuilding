import { describe, expect, it } from "vitest";
import { lightColour, lightWords, needsAttention, poseForState } from "./animation";
import type { FloorLight, SessionState } from "../types";

const STATES: readonly SessionState[] = ["idle", "thinking", "running", "waiting-human", "error", "done"];
const LIGHTS: readonly FloorLight[] = ["green", "yellow", "red", "off"];

describe("poseForState", () => {
  it("gives every state its own pose", () => {
    expect(new Set(STATES.map(poseForState)).size).toBe(STATES.length);
  });
  it("maps the headline states", () => {
    expect(poseForState("running")).toBe("typing");
    expect(poseForState("waiting-human")).toBe("hand-up");
    expect(poseForState("error")).toBe("stuck");
    expect(poseForState("done")).toBe("finished");
  });
});

describe("needsAttention", () => {
  it("is true only for waiting-human and error", () => {
    expect(STATES.filter(needsAttention)).toEqual(["waiting-human", "error"]);
  });
});

describe("lights", () => {
  it("use the DESIGN.md status tokens", () => {
    expect(LIGHTS.map(lightColour)).toEqual(["var(--ok)", "var(--wait)", "var(--fault)", "var(--off)"]);
  });
  it("have plain words for screen readers", () => {
    expect(lightWords("yellow")).toBe("needs you");
    expect(new Set(LIGHTS.map(lightWords)).size).toBe(LIGHTS.length);
  });
});
