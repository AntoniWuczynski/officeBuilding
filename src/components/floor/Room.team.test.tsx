import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { makeSeedSnapshot } from "../../api/mockData";
import { ROOM, partitionY, roomDeskSlot, seatOf, zoneDeskSlot } from "../../lib/layout";
import type { OfficeSnapshot, Session } from "../../types";
import { boundTo, floor, managerSession, withManager } from "./floorHarness";

const own = ["h-1", "h-2", "h-3", "h-4"];
const hired = ["t-1", "t-2", "t-3", "t-4", "t-5"];

/** p-notes with A Human's four desks and the manager's five, the manager having also sent a note to h-2. */
function crowded(hiredIds: readonly string[] = hired): OfficeSnapshot {
  const seed = makeSeedSnapshot();
  const desk = (id: string): Session => ({ ...managerSession("p-notes", id), title: id });
  const withBoss = withManager(seed, "p-notes", (m) => ({ ...boundTo("m-1")(m), managed: [...hiredIds, "h-2"], hired: hiredIds }));
  return { ...withBoss, sessions: [...withBoss.sessions, managerSession("p-notes"), ...own.map(desk), ...hired.map(desk)] };
}

/** Where a worker's seat stands, as its billboard's left and top. */
function seatOfWorker(id: string): { readonly x: number; readonly y: number } {
  const button = document.querySelector<HTMLElement>(`[data-testid=worker][data-session=${id}]`);
  const at = button?.parentElement;
  if (at === null || at === undefined) throw new Error(`no worker ${id}`);
  return { x: parseFloat(at.style.left), y: parseFloat(at.style.top) };
}

describe("the manager's team zone on a floor", () => {
  it("draws the plaque and a named group holding only the desks the manager hired", () => {
    render(floor(crowded(), "p-notes"));
    expect(screen.getByTestId("manager-zone")).toHaveTextContent("Manager's team");
    const zone = screen.getByRole("group", { name: /^Manager's team/ });
    const inZone = within(zone).getAllByTestId("worker").map((w) => w.getAttribute("data-session"));
    expect(inZone).toEqual(hired);
    expect(screen.getAllByTestId("worker")).toHaveLength(own.length + hired.length);
  });

  it("seats the hired desks in the zone and A Human's below the partition, with the hire slot in the grid", () => {
    render(floor(crowded(), "p-notes"));
    const line = partitionY(hired.length);
    if (line === null) throw new Error("no partition");
    hired.forEach((id, i) => expect(seatOfWorker(id)).toEqual(seatOf(zoneDeskSlot(i))));
    own.forEach((id, i) => {
      expect(seatOfWorker(id)).toEqual(seatOf(roomDeskSlot(i, hired.length)));
      expect(seatOfWorker(id).y).toBeGreaterThan(line);
    });
    const slot = roomDeskSlot(own.length, hired.length);
    expect(screen.getByTestId("hire-desk").parentElement).toHaveStyle({ left: `${slot.x + 18}px`, top: `${slot.y}px` });
    expect(slot.y).toBeGreaterThan(line);
    expect(slot.x + 36).toBeLessThanOrEqual(ROOM.width);
  });

  it("puts the zone's desks before A Human's in document order, so focus runs back to front", () => {
    render(floor(crowded(), "p-notes"));
    const order = screen.getAllByTestId("worker").map((w) => w.getAttribute("data-session"));
    expect(order).toEqual([...hired, ...own]);
  });

  it("keeps a desk the manager only sent a note to among A Human's, still marked managed", () => {
    render(floor(crowded(), "p-notes"));
    expect(within(screen.getByRole("group", { name: /^Manager's team/ })).queryByText("h-2")).not.toBeInTheDocument();
    expect(seatOfWorker("h-2")).toEqual(seatOf(roomDeskSlot(1, hired.length)));
    expect(document.querySelector("[data-testid=worker][data-session=h-2]")?.getAttribute("aria-label")).toContain("managed by the floor's manager");
  });

  it("has no zone, no plaque and the old grid when the manager hired nothing", () => {
    render(floor(crowded([]), "p-notes"));
    expect(screen.queryByTestId("manager-zone")).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: /^Manager's team/ })).not.toBeInTheDocument();
    [...own, ...hired].forEach((id, i) => expect(seatOfWorker(id)).toEqual(seatOf(roomDeskSlot(i))));
  });

  it("draws the same zone inside an island", () => {
    render(floor(crowded(), "p-notes", { look: "island" }));
    expect(screen.getByTestId("manager-zone")).toBeInTheDocument();
    hired.forEach((id, i) => expect(seatOfWorker(id)).toEqual(seatOf(zoneDeskSlot(i))));
  });
});
