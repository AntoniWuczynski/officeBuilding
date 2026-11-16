import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BuildingView } from "./BuildingView";
import { makeSeedSnapshot } from "../../api/mockData";
import type { ViewMode } from "../../state/useViewMode";

// Brittany's rule: what the views can do belongs to both; only the skin differs. Each test drives the same action
// through the real BuildingView in each view, from the one snapshot, and the last one compares what each offers.
const MODES: readonly ViewMode[] = ["office", "islands"];

function setup(mode: ViewMode) {
  const spies = {
    onEnter: vi.fn(),
    onOpenSession: vi.fn(),
    onAnswer: vi.fn(() => Promise.resolve()),
    renameFloor: vi.fn(),
    moveSession: vi.fn(),
  };
  render(
    <BuildingView
      snapshot={makeSeedSnapshot()}
      mode={mode}
      onEnter={spies.onEnter}
      canPickFolder={false}
      onPickFolder={() => Promise.resolve(null)}
      onAddFloor={(path) => Promise.resolve({ id: "p-new", name: path, path, sessionIds: [] })}
      onDemolishFloor={() => Promise.resolve()}
      loadUsage={() => Promise.resolve([])}
      onOpenPlate={() => {}}
      onOpenSession={spies.onOpenSession}
      overrideActions={{ renameFloor: spies.renameFloor, moveSession: spies.moveSession }}
      onAnswer={spies.onAnswer}
    />,
  );
  return spies;
}

/** Opens the corner_shop place panel the way each view does: through its nameplate. */
async function openPanel(mode: ViewMode): Promise<HTMLElement> {
  const user = userEvent.setup();
  const plate = await screen.findByRole("button", { name: /^corner_shop: .* (Go to floor|Show island)\.$/ });
  await user.click(plate);
  expect(mode === "office" ? plate.closest("[data-testid=storey]") : plate.closest(".arch-list-row")).not.toBeNull();
  return screen.getByTestId("place-panel");
}

describe.each(MODES)("shared capabilities in the %s view", (mode) => {
  it("renames a place from its panel", async () => {
    const spies = setup(mode);
    const panel = await openPanel(mode);
    const user = userEvent.setup();
    await user.click(within(panel).getByTestId("roof-sign"));
    await user.keyboard("{Control>}a{/Control}Home base{Enter}");
    expect(spies.renameFloor).toHaveBeenCalledWith("p-shop", "Home base");
  });

  it("moves an agent to another place from its panel", async () => {
    const spies = setup(mode);
    const panel = await openPanel(mode);
    const user = userEvent.setup();
    await user.selectOptions(within(panel).getByRole("combobox", { name: /^Move map-tiles mirror to another/ }), "p-arcade");
    expect(spies.moveSession).toHaveBeenCalledWith("s-3", "p-arcade");
  });

  it("opens an agent's session from its panel row", async () => {
    const spies = setup(mode);
    const panel = await openPanel(mode);
    await userEvent.setup().click(within(panel).getByRole("button", { name: /orders ingest fix/ }));
    expect(spies.onOpenSession).toHaveBeenCalledWith(expect.objectContaining({ id: "s-2" }));
  });

  it("enters the place from its panel", async () => {
    const spies = setup(mode);
    const panel = await openPanel(mode);
    await userEvent.setup().click(within(panel).getByTestId("place-enter"));
    expect(spies.onEnter).toHaveBeenCalledWith("p-shop", expect.anything());
  });

  it("lists what waits on you, including a decisions-file call on a place with no live ask, and answers it", async () => {
    const spies = setup(mode);
    const user = userEvent.setup();
    await screen.findAllByTestId(mode === "office" ? "floor-nameplate" : "island-nameplate");
    await user.click(screen.getByTestId("needs-you"));
    const rows = screen.getAllByTestId("needs-you-row");
    // Live asks (newest first) before the decisions-file call.
    expect(rows.map((r) => r.getAttribute("data-question"))).toEqual(["q-1", "q-2", "q-3"]);
    const file = rows[2] as HTMLElement;
    await user.click(within(file).getByRole("button", { name: "Answer" }));
    await user.click(within(file).getByRole("button", { name: /^A:/ }));
    expect(spies.onAnswer).toHaveBeenCalledWith("q-3", { kind: "option", optionId: "o-5" });
  });
});

describe("what the two views offer", () => {
  /** The shared controls found on a place's panel and the top bar. */
  async function inventory(mode: ViewMode): Promise<Record<string, unknown>> {
    setup(mode);
    const panel = await openPanel(mode);
    return {
      rename: within(panel).queryAllByTestId("roof-sign").length,
      move: within(panel).queryAllByTestId("place-move").length,
      open: within(panel).queryAllByTestId("place-row").length,
      enter: within(panel).queryAllByTestId("place-enter").length,
      needsYou: screen.queryAllByTestId("needs-you").length,
      remove: screen.getAllByRole("button", { name: /^(Demolish|Sink) / }).length,
    };
  }

  it("is the same set of controls, from one snapshot", async () => {
    const office = await inventory("office");
    document.body.innerHTML = "";
    const islands = await inventory("islands");
    expect(office).toEqual(islands);
    expect(office).toMatchObject({ rename: 1, move: 3, open: 3, enter: 1, needsYou: 1 });
  });
});
