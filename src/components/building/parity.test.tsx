import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BuildingView } from "./BuildingView";
import { BoardsContext } from "./BoardsContext";
import { makeSeedSnapshot } from "../../api/mockData";
import { SeenContext, useSeen } from "../../state/useSeen";
import type { ViewMode } from "../../state/useViewMode";

// The parity rule: what the views can do belongs to both; only the skin differs. Each test drives the same action
// through the real BuildingView in each view, from the one snapshot, and the last one compares what each offers.
const MODES: readonly ViewMode[] = ["office", "islands"];

/** What App provides around the building: the seen marks (the real hook) and the tags linked to each place. */
function Around({ link, children }: { readonly link: (projectId: string, tags: readonly string[]) => void; readonly children: React.ReactNode }): React.ReactElement {
  const seen = useSeen();
  return (
    <SeenContext value={seen}>
      <BoardsContext value={{ linked: {}, link }}>{children}</BoardsContext>
    </SeenContext>
  );
}

function setup(mode: ViewMode, setupProjectId?: string) {
  // A news baseline older than the seed data, so its idle agents count as new.
  localStorage.setItem("office-building:seen-since", "0");
  const spies = {
    onEnter: vi.fn(),
    onOpenSession: vi.fn(),
    onAnswer: vi.fn(() => Promise.resolve()),
    renameFloor: vi.fn(),
    moveSession: vi.fn(),
    onTick: vi.fn(),
    link: vi.fn(),
  };
  render(
    <Around link={spies.link}>
      <BuildingView
        snapshot={{ ...makeSeedSnapshot(), ...(setupProjectId === undefined ? {} : { setupProjectId }) }}
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
        onTick={spies.onTick}
      />
    </Around>,
  );
  return spies;
}

/** Opens the corner_shop place panel the way each view does: through its nameplate. */
async function openPanel(mode: ViewMode): Promise<HTMLElement> {
  const user = userEvent.setup();
  const plate = await screen.findByRole("button", { name: /^corner_shop: .* (Show floor|Show island)\.$/ });
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

  it("opens the remove dialog from its panel, and offers no removal on the Setup place", async () => {
    setup(mode);
    const panel = await openPanel(mode);
    await userEvent.setup().click(within(panel).getByTestId("place-remove"));
    expect(within(screen.getByRole("dialog")).getByRole("heading", { name: mode === "office" ? "Demolish corner_shop?" : "Sink corner_shop?" })).toBeInTheDocument();
    document.body.innerHTML = "";
    setup(mode, "p-shop");
    expect(within(await openPanel(mode)).queryByTestId("place-remove")).not.toBeInTheDocument();
  });

  it("cancels a rename on Escape and leaves the panel open", async () => {
    setup(mode);
    const panel = await openPanel(mode);
    const user = userEvent.setup();
    await user.click(within(panel).getByTestId("roof-sign"));
    await user.keyboard("Typed over{Escape}");
    expect(within(panel).queryByTestId("roof-sign-input")).not.toBeInTheDocument();
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
  });

  it("clears a half-typed tag on Escape before the panel closes", async () => {
    setup(mode);
    const panel = await openPanel(mode);
    const user = userEvent.setup();
    const field = within(panel).getByRole("textbox", { name: /Link a board tag/ });
    await user.type(field, "half{Escape}");
    expect(field).toHaveValue("");
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });

  it("puts the panel away on Escape, or on its Close button", async () => {
    setup(mode);
    const user = userEvent.setup();
    await openPanel(mode);
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
    await openPanel(mode);
    await user.click(within(screen.getByTestId("place-panel")).getByRole("button", { name: "Close" }));
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });

  it.each([
    ["needs-you", "Waiting on you"],
    ["boards", "Shared boards"],
  ])("closes the %s popover on the first Escape and the panel under it on the second", async (button, label) => {
    setup(mode);
    const user = userEvent.setup();
    await openPanel(mode);
    await user.click(screen.getByTestId(button));
    expect(screen.getByRole("region", { name: label })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: label })).not.toBeInTheDocument();
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("place-panel")).not.toBeInTheDocument();
  });

  it("closes only the dialog on an Escape pressed inside it, over an open popover", async () => {
    setup(mode);
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("needs-you"));
    await user.click(screen.getByTestId(mode === "office" ? "add-floor" : "add-island"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Waiting on you" })).toBeInTheDocument();
  });

  it("opens the Boards sheet and ticks one of its items", async () => {
    const spies = setup(mode);
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("boards"));
    const sheet = screen.getByRole("region", { name: "Shared boards" });
    expect(within(sheet).getAllByTestId("todo-check")).toHaveLength(3);
    await user.click(within(sheet).getByRole("checkbox", { name: /Renew the domain/ }));
    expect(spies.onTick).toHaveBeenCalledWith("board:~/Desktop/MY-ACTIONS.md|3");
  });

  it("links a board tag to the place from its panel", async () => {
    const spies = setup(mode);
    const panel = await openPanel(mode);
    const user = userEvent.setup();
    await user.type(within(panel).getByRole("textbox", { name: /Link a board tag/ }), "Ops{Enter}");
    expect(spies.link).toHaveBeenCalledWith("p-shop", ["Ops"]);
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
      boards: screen.queryAllByTestId("boards").length,
      newCount: screen.queryAllByTestId("new-count").length,
      newDots: within(panel).queryAllByTestId("new-dot").length,
      replies: within(panel).queryAllByTestId("place-reply").length,
      tags: within(panel).queryAllByRole("textbox", { name: /Link a board tag/ }).length,
      remove: screen.getAllByRole("button", { name: /^(Demolish|Sink) / }).length,
    };
  }

  it("is the same set of controls, from one snapshot", async () => {
    const office = await inventory("office");
    document.body.innerHTML = "";
    const islands = await inventory("islands");
    expect(office).toEqual(islands);
    expect(office).toMatchObject({ rename: 1, move: 3, open: 3, enter: 1, needsYou: 1, boards: 1, newCount: 1, newDots: 2, replies: 2, tags: 1 });
  });
});
