import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { makeSeedSnapshot } from "../../api/mockData";
import { floor } from "./floorHarness";

describe("a Codex desk's helpers on the floor", () => {
  it("draws a tiny desk for each of the demo Codex desk's two helpers, and the waiting one has the waiting light", () => {
    const seed = makeSeedSnapshot();
    render(floor(seed, "p-shop"));
    const pods = screen.getAllByTestId("pod");
    const pod = pods.find((p) => within(p).queryAllByTestId("pod-helper").length === 2);
    expect(pod).toBeDefined();
    expect((pod as HTMLElement).querySelectorAll(".pod-led.led-waiting-human")).toHaveLength(1);
  });
});
