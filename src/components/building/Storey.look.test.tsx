import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { makeSeedSnapshot } from "../../api/mockData";
import type { Session } from "../../types";
import { Storey } from "./Storey";

const seed = makeSeedSnapshot();

function storey(sessions: readonly Session[]): React.ReactElement {
  const project = seed.projects.find((p) => p.id === "p-shop");
  if (project === undefined) throw new Error("no p-shop");
  return (
    <Storey
      level={1}
      width={420}
      floor={{ project, sessions, light: "green", counts: { active: 1, total: sessions.length }, pending: 0, dimmed: false, plant: false, stamp: null, cluster: null }}
      phase="here"
      animate={false}
      onEnter={() => {}}
      onOpenSession={() => {}}
      onShow={() => {}}
      panelOpen={false}
      onDemolish={() => {}}
      onGone={() => {}}
    />
  );
}

describe("a storey's seats", () => {
  it("shows the worker of the session's model and its desk in the tool's wood, never the manager's", () => {
    const shop = seed.sessions.filter((s) => s.projectId === "p-shop");
    const { container } = render(storey(shop));
    const seats = screen.getAllByTestId("storey-seat");
    expect(seats.map((s) => s.querySelector("[data-look]")?.getAttribute("data-look"))).toEqual(["otto-high", "vera-xhigh", expect.stringMatching(/^(theo|mara|walt|ayo|dev|fern)$/)]);
    const woods = [...container.querySelectorAll("[class*='wood-']")].flatMap((e) => [...e.classList].filter((c) => c.startsWith("wood-")));
    expect(woods.sort()).toEqual(["wood-maple", "wood-walnut", "wood-wenge"]);
    expect(container.querySelectorAll(".iso-box").length).toBeGreaterThan(woods.length);
  });
});
