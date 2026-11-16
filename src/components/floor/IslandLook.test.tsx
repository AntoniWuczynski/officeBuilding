import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeSeedSnapshot } from "../../api/mockData";
import type { OfficeSnapshot, Session } from "../../types";
import { FLOOR_CAMERA, project } from "../../lib/iso";
import { ROOM, roomDepth, roomDeskSlot, seatOf } from "../../lib/layout";
import { floor } from "./floorHarness";

// Measured in the app: a seated worker's name tag tops out 52.6px above its seat, its "managed" mark (the tallest tag it
// carries) 69.9px, and the hire sign is 23.7px tall.
const NAME_TAG_TOP = 52.6;
const MANAGED_TOP = 69.9;
const HIRE_SIGN_H = 23.7;

const huts = (): Element[] => [...document.querySelectorAll("[data-testid=hut]")];
const monitors = (): Element[] => [...document.querySelectorAll(".desk-monitor")];

/** A billboard's world anchor, read back from the style Billboard gives it. */
function anchor(el: Element | null): { readonly x: number; readonly y: number; readonly z: number } {
  const b = el?.closest<HTMLElement>(".billboard");
  if (b === null || b === undefined) throw new Error("not on a billboard");
  const z = /translateZ\(([-\d.]+)px\)/.exec(b.style.transform)?.[1];
  return { x: parseFloat(b.style.left), y: parseFloat(b.style.top), z: Number(z) };
}

/** Screen height of a world point; up is negative. */
const screenY = (p: { readonly x: number; readonly y: number; readonly z: number }): number => project(FLOOR_CAMERA, 0, 0, p.x, p.y, p.z).y;

/** corner_shop with one more seated agent, so a worker sits at the desk beside the manager's office. */
function shopWithThreeDesks(): OfficeSnapshot {
  const seed = makeSeedSnapshot();
  const base = seed.sessions.find((s) => s.id === "s-3");
  if (base === undefined) throw new Error("no s-3");
  const extra: Session = { ...base, id: "s-9", title: "a third desk", state: "running" };
  return {
    ...seed,
    sessions: [...seed.sessions, extra],
    projects: seed.projects.map((p) => (p.id === "p-shop" ? { ...p, sessionIds: [...p.sessionIds, "s-9"] } : p)),
  };
}

describe("an island's inside", () => {
  it("puts every agent in a hut, with no monitors, and the manager in a thatched hut", () => {
    render(floor(makeSeedSnapshot(), "p-shop", { look: "island" }));
    expect(screen.getAllByTestId("worker")).toHaveLength(3);
    expect(huts()).toHaveLength(3);
    expect(monitors()).toHaveLength(0);
    expect(screen.getByTestId("manager-hut")).toBeInTheDocument();
    expect(screen.getAllByTestId("camp-palm").length).toBeGreaterThan(0);
  });

  it("keeps the office's desks and monitors in the office", () => {
    render(floor(makeSeedSnapshot(), "p-shop"));
    expect(huts()).toHaveLength(0);
    expect(screen.queryByTestId("manager-hut")).not.toBeInTheDocument();
    expect(screen.queryByTestId("beach")).not.toBeInTheDocument();
    expect(monitors().length).toBeGreaterThan(0);
  });

  it("dresses the manager for the beach, not in the suit", () => {
    const torso = (): string => screen.getByTestId("manager-worker").querySelector<HTMLImageElement>(".pp-torso > img")?.src ?? "";
    const seed = makeSeedSnapshot();
    const { unmount } = render(floor(seed, "p-shop"));
    expect(torso()).toMatch(/boss\/torso/);
    unmount();
    render(floor(seed, "p-shop", { look: "island" }));
    expect(torso()).not.toMatch(/boss\/torso/);
  });

  it("still opens an agent when its person in a hut is clicked", async () => {
    const opened: string[] = [];
    render(floor(makeSeedSnapshot(), "p-shop", { look: "island", onOpenSession: (s) => opened.push(s.id) }));
    const person = screen.getAllByTestId("worker").find((w) => w.getAttribute("data-session") === "s-2");
    if (person === undefined) throw new Error("no s-2");
    expect(person.closest(".group")?.querySelector("[data-testid=hut]")).not.toBeNull();
    await userEvent.click(person);
    expect(opened).toEqual(["s-2"]);
  });

  it("hangs each roof's front edge above its worker's name tag", () => {
    render(floor(makeSeedSnapshot(), "p-shop", { look: "island" }));
    const person = screen.getAllByTestId("worker").find((w) => w.getAttribute("data-session") === "s-2");
    const hut = person?.closest(".group")?.querySelector("[data-testid=hut]");
    if (person === undefined || hut === null || hut === undefined) throw new Error("no hut for s-2");
    const foot = screenY(anchor(hut.querySelector("svg")));
    // The roof is the last two faces drawn; its lowest point is its front edge.
    const roof = [...hut.querySelectorAll("polygon")].slice(-2);
    const lowest = Math.max(...roof.flatMap((p) => (p.getAttribute("points") ?? "").split(" ").map((xy) => Number(xy.split(",")[1]))));
    expect(foot + lowest).toBeLessThanOrEqual(screenY(anchor(person)) - NAME_TAG_TOP);
  });

  it("draws the beach inside the scene, round every corner of the room", () => {
    render(floor(makeSeedSnapshot(), "p-shop", { look: "island" }));
    const beach = screen.getByTestId("beach");
    expect(beach.closest(".world")).not.toBeNull();
    const rx = parseFloat(beach.style.width) / 2;
    const ry = parseFloat(beach.style.height) / 2;
    const cx = parseFloat(beach.style.left) + rx;
    const cy = parseFloat(beach.style.top) + ry;
    const depth = roomDepth(3, true);
    for (const [x, y] of [[0, 0], [ROOM.width, 0], [0, depth], [ROOM.width, depth]] as const) {
      expect(((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2, `corner ${x},${y}`).toBeLessThan(1);
    }
  });
});

describe("the manager's tags", () => {
  for (const look of ["office", "island"] as const) {
    it(`hang above the hire sign at the desk beside the office (${look})`, () => {
      render(floor(makeSeedSnapshot(), "p-shop", { look }));
      const hire = anchor(screen.getByTestId("hire-desk"));
      expect(hire.x).toBe(roomDeskSlot(ROOM.columns - 1).x + 18);
      expect(screenY(anchor(screen.getByTestId("object-decisions")))).toBeLessThanOrEqual(screenY(hire) - HIRE_SIGN_H);
    });

    it(`hang above the tallest tag of a worker at the desk beside the office (${look})`, () => {
      render(floor(shopWithThreeDesks(), "p-shop", { look }));
      const seat = seatOf(roomDeskSlot(ROOM.columns - 1));
      const top = screenY({ ...seat, z: ROOM.slab }) - MANAGED_TOP;
      expect(screenY(anchor(screen.getByTestId("object-decisions")))).toBeLessThanOrEqual(top);
    });
  }
});
