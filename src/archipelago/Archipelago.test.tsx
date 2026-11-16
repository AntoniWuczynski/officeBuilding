import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Archipelago } from "./Archipelago";
import { makeSeedSnapshot } from "../api/mockData";
import { floorLight, matchesQuery, qaFor, questionsForProject, sessionCounts, sessionsForProject } from "../state/selectors";
import type { Present } from "../state/usePresence";
import type { FloorModel } from "../components/building/Storey";
import { MAX_BUILDING_NAME } from "../state/useBuildingName";

const snapshot = makeSeedSnapshot();

/** The floors as the building view makes them, one island each; `sinking` names the project ids that are sinking. */
function entries(sinking: readonly string[] = []): Present<FloorModel>[] {
  return snapshot.projects.map((project) => {
    const sessions = sessionsForProject(snapshot, project.id);
    const qa = qaFor(snapshot, project.id);
    const item: FloorModel = {
      project,
      sessions,
      light: floorLight(sessions),
      counts: sessionCounts(sessions),
      pending: questionsForProject(snapshot, project.id).length,
      dimmed: !matchesQuery(project, ""),
      plant: project.id === snapshot.setupProjectId,
      stamp: qa?.enabled === true ? qa.stamp : null,
    };
    return { key: project.id, item, phase: sinking.includes(project.id) ? "leaving" : "here" };
  });
}

function show(list: Present<FloorModel>[], onRenameIsland = vi.fn()): ReturnType<typeof vi.fn> {
  render(
    <Archipelago
      snapshot={snapshot}
      entries={list}
      onGone={() => {}}
      animate={false}
      onEnter={() => {}}
      onOpenSession={() => {}}
      onRenameIsland={onRenameIsland}
      onMoveSession={() => {}}
      onAddFloor={() => {}}
      onDemolish={() => {}}
    />,
  );
  return onRenameIsland;
}

const shop = snapshot.projects.find((p) => p.name === "corner_shop");
const arcade = snapshot.projects.find((p) => p.name === "arcade");

describe("island panel", () => {
  it("renames through the shared sign, which stops at the name length limit", async () => {
    const user = userEvent.setup();
    const onRename = show(entries());
    await user.click(screen.getByRole("button", { name: /^corner_shop: .* Show island\.$/ }));
    const panel = screen.getByRole("complementary", { name: "corner_shop island" });
    await user.click(within(panel).getByRole("button", { name: "corner_shop" }));
    const field = within(panel).getByRole("textbox", { name: "Island name" });
    expect(field).toHaveAttribute("maxlength", String(MAX_BUILDING_NAME));
    await user.keyboard(`{Control>}a{/Control}${"x".repeat(MAX_BUILDING_NAME + 10)}{Enter}`);
    expect(onRename).toHaveBeenCalledWith(shop?.id, "x".repeat(MAX_BUILDING_NAME));
  });

  it("offers an agent only islands it can move to: not the Setup island, not one that is sinking, not its own", async () => {
    const user = userEvent.setup();
    if (shop === undefined || arcade === undefined) throw new Error("seed has no corner_shop and arcade");
    show(entries([arcade.id]));
    await user.click(screen.getByRole("button", { name: /^corner_shop: .* Show island\.$/ }));
    const panel = screen.getByRole("complementary", { name: "corner_shop island" });
    const menu = within(panel).getAllByRole("combobox")[0];
    if (menu === undefined) throw new Error("no move menu");
    const labels = within(menu).getAllByRole("option").map((o) => o.textContent);
    const others = snapshot.projects.filter((p) => p.id !== shop.id && p.id !== arcade.id && p.id !== snapshot.setupProjectId).map((p) => p.name);
    expect(labels).toEqual(["Move to...", ...others]);
    expect(labels).not.toContain("Setup");
    expect(labels).not.toContain("arcade");
  });
});
