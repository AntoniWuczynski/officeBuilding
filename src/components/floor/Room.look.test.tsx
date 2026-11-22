import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { makeSeedSnapshot } from "../../api/mockData";
import type { OfficeSnapshot, Session } from "../../types";
import { UltracodeContext } from "../../state/UltracodeContext";
import { boundTo, floor, managerSession, withManager } from "./floorHarness";

const looksOf = (el: ParentNode): string[] => [...el.querySelectorAll("[data-look]")].map((e) => e.getAttribute("data-look") ?? "");
const woodsOf = (selector: string): string[] => [...document.querySelectorAll(selector)].flatMap((e) => [...e.classList].filter((c) => c.startsWith("wood-")));
const workerOf = (id: string): HTMLElement => {
  const el = document.querySelector<HTMLElement>(`[data-testid=worker][data-session=${id}]`);
  if (el === null) throw new Error(`no worker ${id}`);
  return el;
};

/** The seed with the shop's sessions changed. */
function shop(change: (s: Session) => Session): OfficeSnapshot {
  const seed = makeSeedSnapshot();
  return { ...seed, sessions: seed.sessions.map(change) };
}

describe("desk wood on a floor", () => {
  it("gives each session's desk its tool's wood", () => {
    render(floor(makeSeedSnapshot(), "p-shop"));
    const woods = woodsOf(".iso-box:not(.pod-desk)");
    expect(woods.sort()).toEqual(["wood-maple", "wood-walnut", "wood-wenge"].sort());
  });

  it("leaves the manager's desk plain", () => {
    const seed = withManager(makeSeedSnapshot(), "p-shop", boundTo("m-1"));
    render(floor({ ...seed, sessions: [...seed.sessions, managerSession("p-shop")] }, "p-shop"));
    expect(woodsOf(".iso-box:not(.pod-desk)")).toHaveLength(3);
    expect(looksOf(document)).toContain("boss");
  });

  it("gives a helper's tiny desk its agent's wood", () => {
    render(floor(makeSeedSnapshot(), "p-shop"));
    const codexPod = screen.getAllByTestId("pod").find((p) => within(p).queryAllByTestId("pod-helper").length === 2);
    expect(codexPod).toBeDefined();
    expect(woodsOf(".pod-desk").length).toBeGreaterThan(0);
    for (const c of (codexPod as HTMLElement).querySelectorAll(".pod-desk")) expect(c).toHaveClass("wood-wenge");
  });

  it("gives an island's crates the wood too", () => {
    render(floor(makeSeedSnapshot(), "p-shop", { look: "island" }));
    expect(woodsOf(".iso-box")).toEqual(expect.arrayContaining(["wood-maple", "wood-wenge", "wood-walnut"]));
  });
});

describe("the look of a floor's workers", () => {
  it("draws a Sonnet desk and a Terra desk at high effort as nell-high", () => {
    const snap = shop((s) => (s.id === "s-1" ? { ...s, model: "claude-sonnet-5-5", effort: "high" } : s.id === "s-2" ? { ...s, model: "gpt-5.6-terra", effort: "high" } : s));
    render(floor(snap, "p-shop"));
    expect(looksOf(workerOf("s-1"))).toEqual(["nell-high"]);
    expect(looksOf(workerOf("s-2"))).toEqual(["nell-high"]);
  });

  it("draws a session with no model as one of the six", () => {
    render(floor(makeSeedSnapshot(), "p-shop"));
    expect(looksOf(workerOf("s-3"))).toHaveLength(1);
    expect(["theo", "mara", "walt", "ayo", "dev", "fern"]).toContain(looksOf(workerOf("s-3"))[0]);
  });

  it("draws a helper by its own model, or its agent's", () => {
    render(floor(makeSeedSnapshot(), "p-shop"));
    const pod = screen.getAllByTestId("pod").find((p) => within(p).queryAllByTestId("pod-helper").length === 2);
    // s-2 is a Codex desk on gpt-6-astra at xhigh. Ada has its own effort, Lin has nothing of its own.
    expect(looksOf(pod as HTMLElement)).toEqual(["vera-high", "vera-xhigh"]);
  });

  it("tints only the Claude Code desk in the ultracode list, and none of its helpers", () => {
    const snap = makeSeedSnapshot();
    render(<UltracodeContext value={new Set(["s-1", "s-2"])}>{floor(snap, "p-shop")}</UltracodeContext>);
    expect(workerOf("s-1").querySelector(".puppet")).toHaveClass("is-ultracode");
    expect(workerOf("s-2").querySelector(".puppet")).not.toHaveClass("is-ultracode");
    expect(workerOf("s-3").querySelector(".puppet")).not.toHaveClass("is-ultracode");
    expect(document.querySelectorAll("[data-testid=pod] .is-ultracode")).toHaveLength(0);
  });

  it("says in the worker's name that only the ultracode desk was hired with ultracode on", () => {
    render(<UltracodeContext value={new Set(["s-1", "s-2"])}>{floor(makeSeedSnapshot(), "p-shop")}</UltracodeContext>);
    expect(workerOf("s-1").getAttribute("aria-label")).toContain("hired with ultracode on");
    expect(workerOf("s-2").getAttribute("aria-label")).not.toContain("ultracode");
    expect(workerOf("s-3").getAttribute("aria-label")).not.toContain("ultracode");
  });

  it("walks a leaver out in the look it sat in", () => {
    const seed = makeSeedSnapshot();
    const { rerender } = render(floor(seed, "p-shop"));
    expect(screen.queryByTestId("walk")).not.toBeInTheDocument();
    const gone: OfficeSnapshot = { ...seed, sessions: seed.sessions.filter((s) => s.id !== "s-1") };
    rerender(floor({ ...gone, projects: gone.projects.map((p) => ({ ...p, sessionIds: p.sessionIds.filter((i) => i !== "s-1") })) }, "p-shop"));
    const leaving = screen.getAllByTestId("walk").find((w) => w.classList.contains("walk-leave") || w.classList.contains("walk-handin"));
    expect(leaving).toBeDefined();
    expect(looksOf(leaving as HTMLElement)).toEqual(["otto-high"]);
  });
});
