import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { BuildingView } from "./BuildingView";
import { makeDemoSnapshot } from "../../api/mockData";
import { EMPTY_MEMORY, groupByProject } from "../../state/clusters";
import { ProjectsContext } from "../../state/ProjectsContext";
import type { ViewMode } from "../../state/useViewMode";
import type { OfficeSnapshot } from "../../types";

const MODES: readonly ViewMode[] = ["office", "islands"];

/** Two repos and the setup floor, the repos' folders being corner_shop (p-shop) and officeBuilding (p-office). */
function snapshot(withRules: boolean): OfficeSnapshot {
  const demo = makeDemoSnapshot();
  const projects = demo.projects.filter((p) => ["p-shop", "p-office", demo.setupProjectId].includes(p.id));
  const projectRules = withRules
    ? [
        { id: "alpha", name: "Alpha", prefixes: [], contains: [], repos: ["officeBuilding"] },
        { id: "beta", name: "Beta", prefixes: [], contains: [], repos: ["corner_shop"] },
      ]
    : [];
  return { ...demo, projects, projectRules };
}

function show(snap: OfficeSnapshot, mode: ViewMode, provide: boolean): React.ReactElement {
  const building = (
    <BuildingView
      onOpenRequest={() => {}}
      snapshot={snap}
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
  if (!provide) return building;
  const { memory: _memory, ...grouping } = groupByProject(snap, EMPTY_MEMORY);
  return <ProjectsContext value={{ ...grouping, rules: snap.projectRules, rulesError: null, similar: () => [], assignProject: () => Promise.resolve() }}>{building}</ProjectsContext>;
}

async function order(mode: ViewMode): Promise<string[]> {
  const plates = await screen.findAllByTestId(mode === "office" ? "floor-nameplate" : "island-nameplate");
  return plates.map((p) => p.getAttribute("data-project") ?? "");
}

/** The office view stands the plant room in the basement, ahead of the floors in the DOM; the islands list ends with its lighthouse. */
const withSetup = (mode: ViewMode, repos: string[], setup: string): string[] => (mode === "office" ? [setup, ...repos] : [...repos, setup]);

describe.each(MODES)("floors by project cluster, %s view", (mode) => {
  it("keeps today's order with no rules or names", async () => {
    const snap = snapshot(false);
    render(show(snap, mode, true));
    expect(await order(mode)).toEqual(withSetup(mode, ["p-shop", "p-office"], snap.setupProjectId));
  });

  it("follows the rule-file order, repos first and the setup floor last", async () => {
    const snap = snapshot(true);
    render(show(snap, mode, true));
    expect(await order(mode)).toEqual(withSetup(mode, ["p-office", "p-shop"], snap.setupProjectId));
  });

  it("keeps today's order with no provider", async () => {
    const snap = snapshot(true);
    render(show(snap, mode, false));
    expect(await order(mode)).toEqual(withSetup(mode, ["p-shop", "p-office"], snap.setupProjectId));
  });
});
