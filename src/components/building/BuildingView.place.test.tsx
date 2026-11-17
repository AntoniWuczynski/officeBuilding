import { describe, expect, it, vi } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BuildingView } from "./BuildingView";
import { makeSeedSnapshot } from "../../api/mockData";
import type { OfficeSnapshot } from "../../types";
import type { ViewMode } from "../../state/useViewMode";

function setup() {
  const onEnter = vi.fn();
  const onOpenSession = vi.fn();
  render(
    <BuildingView
      snapshot={makeSeedSnapshot()}
      onEnter={onEnter}
      canPickFolder={false}
      onPickFolder={() => Promise.resolve(null)}
      onAddFloor={(path) => Promise.resolve({ id: "p-new", name: path, path, sessionIds: [] })}
      onDemolishFloor={() => Promise.resolve()}
      loadUsage={() => Promise.resolve([])}
      onOpenPlate={() => {}}
      onOpenSession={onOpenSession}
      overrideActions={{ renameFloor: () => {}, moveSession: () => {} }}
      onAnswer={() => Promise.resolve()}
      onTick={() => {}}
    />,
  );
  const storey = (id: string): HTMLElement => screen.getAllByTestId("storey").find((s) => s.getAttribute("data-project") === id) as HTMLElement;
  return { onEnter, onOpenSession, storey };
}

describe("an office floor's nameplate", () => {
  it("opens the place panel without going in, and the panel's Enter goes in", async () => {
    const { onEnter, storey } = setup();
    const user = userEvent.setup();
    const plate = within(storey("p-shop")).getByTestId("floor-nameplate");
    expect(plate).toHaveAttribute("aria-expanded", "false");
    await user.click(plate);
    expect(onEnter).not.toHaveBeenCalled();
    const panel = screen.getByRole("complementary", { name: "corner_shop floor" });
    expect(plate).toHaveAttribute("aria-expanded", "true");
    expect(plate).toHaveAttribute("aria-controls", panel.id);
    expect(within(panel).getByRole("heading", { name: /corner_shop/ })).toHaveFocus();
    await user.click(within(panel).getByRole("button", { name: "Enter floor" }));
    expect(onEnter).toHaveBeenCalledWith("p-shop", expect.anything());
  });

  it("leaves the floor body entering the floor, as before", async () => {
    const { onEnter, storey } = setup();
    await userEvent.setup().click(storey("p-arcade"));
    expect(onEnter).toHaveBeenCalledWith("p-arcade", expect.anything());
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });

  it("puts the panel away on Escape or Close, and stands the usage signs aside while it is open", async () => {
    const { storey } = setup();
    const user = userEvent.setup();
    await user.click(within(storey("p-shop")).getByTestId("floor-nameplate"));
    expect(document.querySelector(".building-body")).toHaveClass("has-place-panel");
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
    await user.click(within(storey("p-shop")).getByTestId("floor-nameplate"));
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });

  it("shows the waiting count on the nameplate and reads it out", () => {
    const { storey } = setup();
    const plate = within(storey("p-shop")).getByTestId("floor-nameplate");
    expect(within(plate).getByLabelText("2 waiting on you")).toHaveTextContent("2");
    expect(plate).toHaveAttribute("aria-label", expect.stringContaining("2 waiting on you"));
  });
});

describe("a person at a desk on the overview", () => {
  it("opens their session without going into the floor", async () => {
    const { onEnter, onOpenSession, storey } = setup();
    await userEvent.setup().click(within(storey("p-arcade")).getByRole("button", { name: /netcode prototype, Idle\. Open it\./ }));
    expect(onOpenSession).toHaveBeenCalledWith(expect.objectContaining({ id: "s-6" }));
    expect(onEnter).not.toHaveBeenCalled();
  });
});

describe("Escape closes one thing at a time", () => {
  it("cancels a rename in the panel heading without closing the panel; a second Escape closes it", async () => {
    const { storey } = setup();
    const user = userEvent.setup();
    await user.click(within(storey("p-shop")).getByTestId("floor-nameplate"));
    const panel = screen.getByTestId("place-panel");
    await user.click(within(panel).getByTestId("roof-sign"));
    await user.keyboard("Typed over{Escape}");
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
    expect(within(panel).queryByTestId("roof-sign-input")).not.toBeInTheDocument();
    expect(within(panel).getByTestId("roof-sign")).toHaveTextContent("corner_shop");
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });

  it("closes the Needs-you list before the panel under it", async () => {
    const { storey } = setup();
    const user = userEvent.setup();
    await user.click(within(storey("p-shop")).getByTestId("floor-nameplate"));
    await user.click(screen.getByTestId("needs-you"));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: "Waiting on you" })).not.toBeInTheDocument();
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });
});

describe("Escape in a dialog or a popover over the panel", () => {
  it("closes only the Add-floor dialog, and the Demolish dialog, leaving the panel open", async () => {
    const { storey } = setup();
    const user = userEvent.setup();
    await user.click(within(storey("p-shop")).getByTestId("floor-nameplate"));
    await user.click(screen.getByTestId("add-floor"));
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("add-floor-dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
    await user.click(within(storey("p-notes")).getByTestId("demolish-floor"));
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("demolish-dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
  });

  it("closes the newest of the Boards sheet and the Needs-you list first, then the other, then the panel", async () => {
    const { storey } = setup();
    const user = userEvent.setup();
    await user.click(within(storey("p-shop")).getByTestId("floor-nameplate"));
    await user.click(screen.getByTestId("boards"));
    await user.click(screen.getByTestId("needs-you"));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: "Waiting on you" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Shared boards" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: "Shared boards" })).not.toBeInTheDocument();
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });
});

describe("the panel does not come back by itself", () => {
  const shown = (snapshot: OfficeSnapshot, mode: ViewMode): React.ReactElement => (
    <BuildingView
      snapshot={snapshot}
      mode={mode}
      onEnter={() => {}}
      canPickFolder={false}
      onPickFolder={() => Promise.resolve(null)}
      onAddFloor={(path) => Promise.resolve({ id: "p-new", name: path, path, sessionIds: [] })}
      onDemolishFloor={() => Promise.resolve()}
      loadUsage={() => Promise.resolve([])}
      onOpenPlate={() => {}}
      onOpenSession={() => {}}
      overrideActions={{ renameFloor: () => {}, moveSession: () => {} }}
      onAnswer={() => Promise.resolve()}
      onTick={() => {}}
    />
  );
  const plate = (): HTMLElement => screen.getAllByTestId("floor-nameplate").find((p) => p.getAttribute("data-project") === "p-shop") as HTMLElement;

  it("stays shut after a switch to Islands and back", async () => {
    const seed = makeSeedSnapshot();
    const { rerender } = render(shown(seed, "office"));
    await userEvent.setup().click(plate());
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
    rerender(shown(seed, "islands"));
    rerender(shown(seed, "office"));
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });

  it("stays shut when its floor is demolished and built again", async () => {
    const seed = makeSeedSnapshot();
    const { rerender } = render(shown(seed, "office"));
    await userEvent.setup().click(plate());
    rerender(shown({ ...seed, projects: seed.projects.filter((p) => p.id !== "p-shop"), sessions: seed.sessions.filter((s) => s.projectId !== "p-shop") }, "office"));
    const rubble = screen.getAllByTestId("storey").find((s) => s.getAttribute("data-project") === "p-shop")?.querySelector(".rubble") ?? document.body;
    act(() => {
      rubble.dispatchEvent(Object.assign(new Event("animationend", { bubbles: true }), { animationName: "rubble-clear" }));
    });
    rerender(shown(seed, "office"));
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });
});
