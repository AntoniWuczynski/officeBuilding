import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeSeedSnapshot } from "../../api/mockData";
import type { ManagerFloor, OfficeSnapshot } from "../../types";
import type { Section } from "./sections";
import { boundTo, floor, managerSession, withManager } from "./floorHarness";

/** p-notes has no desks, so the manager's desk light is the only one on the floor. */
function notesWithManager(over: Partial<ManagerFloor> = {}): OfficeSnapshot {
  const seed = makeSeedSnapshot();
  const hired = withManager(seed, "p-notes", (m) => ({ ...boundTo("m-1")(m), ...over }));
  return { ...hired, sessions: [...hired.sessions, managerSession("p-notes")] };
}

describe("the manager's office", () => {
  it("shows a sign and no figure when nobody is hired, and the sign opens the Manager section", async () => {
    const opened: (Section | null)[] = [];
    render(floor(makeSeedSnapshot(), "p-notes", { onSection: (s) => opened.push(s) }));
    expect(screen.queryByTestId("manager-worker")).not.toBeInTheDocument();
    const sign = screen.getByTestId("object-manager");
    expect(sign).toHaveTextContent("Hire a manager");
    expect(sign).toHaveAccessibleName("Manager's desk: nobody is hired. Open Manager.");
    await userEvent.click(sign);
    expect(opened).toEqual(["manager"]);
  });

  it("has neither a sign nor a figure in the plant room", () => {
    render(floor({ ...makeSeedSnapshot(), setupProjectId: "p-notes" }, "p-notes"));
    expect(screen.queryByTestId("object-manager")).not.toBeInTheDocument();
    expect(screen.queryByTestId("manager-worker")).not.toBeInTheDocument();
    expect(screen.getByTestId("object-decisions")).toBeInTheDocument();
  });

  it("draws the figure and opens the Manager section when a manager is hired", async () => {
    const opened: (Section | null)[] = [];
    render(floor(notesWithManager(), "p-notes", { onSection: (s) => opened.push(s) }));
    expect(screen.queryByTestId("object-manager")).not.toBeInTheDocument();
    const boss = screen.getByTestId("manager-worker");
    expect(boss).toHaveAccessibleName("The floor's manager: Working. Open Manager.");
    await userEvent.click(boss);
    expect(opened).toEqual(["manager"]);
  });

  it("never seats the manager's session at an ordinary desk", () => {
    render(floor(notesWithManager(), "p-notes"));
    expect(screen.queryAllByTestId("worker")).toHaveLength(0);
    expect(screen.getByTestId("hire-desk").parentElement).toHaveStyle({ left: "46px" });
  });

  it("lights the desk green while it works, yellow while a call is open and off while paused", () => {
    const led = (): string => document.querySelector(".led")?.className ?? "";
    const { unmount } = render(floor(notesWithManager(), "p-notes"));
    expect(led()).toContain("led-running");
    unmount();
    const escalated = render(floor(notesWithManager({ escalated: true }), "p-notes"));
    expect(led()).toContain("led-waiting-human");
    escalated.unmount();
    render(floor(notesWithManager({ paused: true }), "p-notes"));
    expect(led()).toContain("led-done");
    expect(screen.getByTestId("manager-worker")).toHaveAccessibleName("The floor's manager: Paused. Open Manager.");
  });

  it("walks a note from the manager's seat while the figure is away", () => {
    const seed = notesWithManager();
    const { rerender } = render(floor(seed, "p-shop"));
    const shop = withManager(seed, "p-shop", boundTo("m-2"));
    const withBoss = { ...shop, sessions: [...shop.sessions, managerSession("p-shop", "m-2")] };
    rerender(floor(withBoss, "p-shop"));
    expect(screen.getByTestId("manager-worker")).toBeInTheDocument();
    expect(screen.queryByTestId("walk")).not.toBeInTheDocument();
    const note = { id: "msg-1", fromSessionId: "m-2", toSessionId: "s-2", text: "start", sentAt: "2026-09-28T15:45:00Z" };
    rerender(floor({ ...withBoss, messages: [note] }, "p-shop"));
    expect(screen.getByTestId("walk")).toHaveClass("walk-note");
    expect(screen.queryByTestId("manager-worker")).not.toBeInTheDocument();
  });
});

describe("managed desks", () => {
  function shop(): OfficeSnapshot {
    return withManager(makeSeedSnapshot(), "p-shop", (m) => ({
      ...m,
      managed: ["s-2"],
      queue: [
        { id: "n-1", toSessionId: "s-2", text: "one", queuedAt: "2026-09-28T15:30:00Z", hold: "busy" },
        { id: "n-2", toSessionId: "s-2", text: "two", queuedAt: "2026-09-28T15:31:00Z", hold: "stuck" },
      ],
    }));
  }

  function deskOf(id: string): HTMLElement {
    const worker = screen.getAllByTestId("worker").find((w) => w.getAttribute("data-session") === id);
    if (worker === undefined) throw new Error(`no worker ${id}`);
    return worker;
  }

  it("marks the desk the manager drives and counts its waiting notes", () => {
    render(floor(shop(), "p-shop"));
    const worker = deskOf("s-2");
    expect(within(worker).getByText("managed")).toBeInTheDocument();
    expect(worker.getAttribute("aria-label")).toContain("managed by the floor's manager, 2 notes from the manager waiting");
    expect(screen.getByTestId("desk-note")).toHaveTextContent("2");
    expect(within(deskOf("s-3")).queryByText("managed")).not.toBeInTheDocument();
  });

  it("says the same in words on the hover card", async () => {
    render(floor(shop(), "p-shop"));
    await userEvent.hover(deskOf("s-2"));
    const card = await screen.findByTestId("worker-card");
    expect(card).toHaveTextContent("Managed by the floor's manager");
    expect(card).toHaveTextContent("2 notes from the manager waiting: it is busy");
  });

  it("keeps a lone note singular", () => {
    const one = withManager(shop(), "p-shop", (m) => ({ ...m, queue: m.queue.slice(0, 1) }));
    render(floor(one, "p-shop"));
    expect(deskOf("s-2").getAttribute("aria-label")).toContain("1 note from the manager waiting");
  });
});
