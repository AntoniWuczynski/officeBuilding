import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BuildingView } from "./BuildingView";
import { makeSeedSnapshot } from "../../api/mockData";

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
