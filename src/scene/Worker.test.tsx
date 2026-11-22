import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BUILDS, TIER_FAMILIES, TIER_WORKERS } from "../lib/modelTiers";
import { lookOf } from "../lib/looks";
import type { Drawn } from "../lib/looks";
import { UltracodeContext } from "../state/UltracodeContext";
import { UltracodeTint, WalkingWorker, Worker, WorkerFace } from "./Worker";

const tiers = TIER_WORKERS.flatMap((worker) => BUILDS.map((build) => [worker, build] as const));

describe("Worker: every tier and build has every rig part", () => {
  it.each(tiers)("%s in the %s build", (worker, build) => {
    const of: Drawn = { id: "s-1", tool: "codex", model: TIER_FAMILIES[worker][1], effort: build };
    const look = lookOf(of);
    expect(look.name).toBe(`${worker}-${build}`);
    for (const part of Object.values(look.parts)) {
      expect(part.w).toBeGreaterThan(0);
      expect(part.h).toBeGreaterThan(0);
    }

    const seated = render(<Worker of={of} pose="typing" />);
    const person = seated.container.querySelector(".person");
    expect(person).toHaveAttribute("data-look", look.name);
    const sat = seated.container.querySelectorAll("img");
    expect(sat).toHaveLength(9);
    for (const img of sat) expect(img.getAttribute("src")).not.toBe("");
    seated.unmount();

    const walking = render(<WalkingWorker of={of} carrying={null} />);
    expect(walking.container.querySelector(".walker")).toHaveAttribute("data-look", look.name);
    const walked = walking.container.querySelectorAll("img");
    expect(walked).toHaveLength(10);
    for (const img of walked) expect(img.getAttribute("src")).not.toBe("");
    walking.unmount();
  });
});

describe("the ultracode tint", () => {
  const claude: Drawn = { id: "s-1", tool: "claude-code", model: "claude-opus-5-5", effort: "high" };
  const codex: Drawn = { id: "s-2", tool: "codex", model: "gpt-6-sol", effort: "high" };
  const outside: Drawn = { id: "s-3", tool: "claude-code", model: null, effort: null };
  const set = new Set(["s-1", "s-2"]);

  const tinted = (of: Drawn, provided: boolean): boolean => {
    const figure = <Worker of={of} pose="idle" />;
    const { container, unmount } = render(provided ? <UltracodeContext value={set}>{figure}</UltracodeContext> : figure);
    const on = container.querySelector(".puppet")?.classList.contains("is-ultracode") ?? false;
    unmount();
    return on;
  };

  it("tints only a Claude Code session in the set", () => {
    expect(tinted(claude, true)).toBe(true);
    expect(tinted(codex, true)).toBe(false);
    expect(tinted(outside, true)).toBe(false);
  });

  it("tints nobody without a provider", () => {
    expect(tinted(claude, false)).toBe(false);
  });

  it("tints a walking worker the same way", () => {
    const { container } = render(
      <UltracodeContext value={set}>
        <WalkingWorker of={claude} carrying="note" />
      </UltracodeContext>,
    );
    expect(container.querySelector(".puppet")).toHaveClass("is-ultracode");
  });

  it("draws the face as a decorative head, tinted under the same rule", () => {
    render(
      <UltracodeContext value={set}>
        <WorkerFace of={claude} />
        <WorkerFace of={codex} />
      </UltracodeContext>,
    );
    const [mine, theirs] = screen.getAllByTestId("worker-face");
    expect(mine).toHaveAttribute("alt", "");
    expect(mine).toHaveAttribute("data-look", "otto-high");
    expect(mine).toHaveAttribute("src", lookOf(claude).parts.head.src);
    expect(mine).toHaveClass("is-ultracode");
    expect(theirs).not.toHaveClass("is-ultracode");
  });

  it("renders the filter once, under its id", () => {
    const { container } = render(<UltracodeTint />);
    expect(container.querySelectorAll("filter#ultracode-tint")).toHaveLength(1);
  });
});
