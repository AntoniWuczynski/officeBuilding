import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PlacePanel, StaffContext } from "./PlacePanel";
import { makeSeedSnapshot } from "../../api/mockData";
import { questionsForProject, sessionsForProject } from "../../state/selectors";

const snapshot = makeSeedSnapshot();
const sessions = sessionsForProject(snapshot, "p-shop");

function show(over: Partial<React.ComponentProps<typeof PlacePanel>> = {}) {
  const spies = { onRename: vi.fn(), onMove: vi.fn(), onOpenSession: vi.fn(), onEnter: vi.fn() };
  render(
    <PlacePanel
      projectId="p-shop"
      name="corner_shop"
      noun="floor"
      enterLabel="Enter floor"
      sessions={sessions}
      counts={{ active: 1, total: 3 }}
      asks={questionsForProject(snapshot, "p-shop")}
      moveTargets={[{ id: "p-arcade", name: "arcade" }]}
      leaving={false}
      className="office-panel"
      {...spies}
      {...over}
    />,
  );
  return spies;
}

describe("the place panel", () => {
  it("lists the most urgent agent first, and shows a waiting agent's ask in its row", () => {
    show();
    const rows = screen.getAllByTestId("place-row");
    // Seed order is audit checkout (working), orders ingest fix (needs you), map-tiles mirror (stuck).
    expect(rows.map((r) => within(r).getByText(/./, { selector: "b" }).textContent)).toEqual(["map-tiles mirror", "orders ingest fix", "audit checkout"]);
    expect(rows[1]).toHaveTextContent("Persist EventType onto entries");
    expect(rows[2]).not.toHaveTextContent("Persist");
  });

  it("keeps the move menu small and beside the row, offering only the targets it was given", () => {
    show();
    const menu = screen.getAllByTestId("place-move")[0] as HTMLElement;
    expect(within(menu).getAllByRole("option").map((o) => o.textContent)).toEqual(["Move to...", "arcade"]);
    expect(menu.closest("li")?.querySelector("[data-testid=place-row]")).not.toBeNull();
  });

  it("has no move menu when there is nowhere to move to", () => {
    show({ moveTargets: [] });
    expect(screen.queryByTestId("place-move")).not.toBeInTheDocument();
  });

  it("cannot enter a place that is going away, and offers Close only when the view allows it", () => {
    show({ leaving: true });
    expect(screen.getByTestId("place-enter")).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
  });

  it("enters and closes", async () => {
    const onClose = vi.fn();
    const { onEnter } = show({ onClose });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Enter floor" }));
    expect(onEnter).toHaveBeenCalledWith("p-shop");
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("offers Sink / Demolish only when given a way to remove, and calls it", async () => {
    show();
    expect(screen.queryByTestId("place-remove")).not.toBeInTheDocument();
    document.body.innerHTML = "";
    const onRemove = vi.fn();
    show({ onRemove });
    await userEvent.setup().click(screen.getByRole("button", { name: "Demolish floor" }));
    expect(onRemove).toHaveBeenCalled();
    document.body.innerHTML = "";
    show({ onRemove, noun: "island", leaving: true });
    expect(screen.getByRole("button", { name: "Sink island" })).toBeDisabled();
  });

  it("offers each agent a Throw out button only when it is given an onExile", async () => {
    show();
    expect(screen.queryByTestId("place-exile")).not.toBeInTheDocument();
    document.body.innerHTML = "";
    const onExile = vi.fn();
    show({ onExile });
    expect(screen.getAllByTestId("place-exile")).toHaveLength(3);
    expect(screen.getByRole("button", { name: /orders ingest fix/ })).not.toHaveAttribute("data-testid", "place-exile");
    await userEvent.setup().click(screen.getAllByTestId("place-exile")[1]!);
    expect(onExile).toHaveBeenCalledWith(expect.objectContaining({ id: "s-2" }));
  });

  it("offers no Throw out button on the place's manager or QA reviewer", () => {
    render(
      <StaffContext value={new Set(["s-2"])}>
        <PlacePanel projectId="p-shop" name="corner_shop" noun="floor" enterLabel="Enter floor" sessions={sessions} counts={{ active: 1, total: 3 }} asks={[]} moveTargets={[]} leaving={false} className="office-panel" onRename={vi.fn()} onMove={vi.fn()} onOpenSession={vi.fn()} onEnter={vi.fn()} onExile={vi.fn()} />
      </StaffContext>,
    );
    expect(screen.getAllByTestId("place-exile")).toHaveLength(2);
    const staffRow = screen.getByRole("button", { name: /orders ingest fix/ }).closest("li");
    expect(staffRow?.querySelector("[data-testid=place-exile]")).toBeNull();
  });
});

describe("the place panel's words", () => {
  it("shows each row's model line and the face of its worker", () => {
    show();
    const rows = screen.getAllByTestId("place-row");
    const audit = rows.find((r) => r.textContent?.includes("audit checkout"));
    expect(audit).toHaveTextContent("Working · Claude Code · claude-opus-5-5, high effort");
    expect(within(audit as HTMLElement).getByTestId("worker-face")).toHaveAttribute("data-look", "otto-high");
    const stuck = rows.find((r) => r.textContent?.includes("map-tiles mirror"));
    expect(stuck).not.toHaveTextContent("effort");
  });
});
