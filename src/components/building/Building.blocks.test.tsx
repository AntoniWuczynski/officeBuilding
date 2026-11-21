import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { BuildingView } from "./BuildingView";
import { makeDemoSnapshot } from "../../api/mockData";
import { EMPTY_MEMORY, groupByProject } from "../../state/clusters";
import { ProjectsContext } from "../../state/ProjectsContext";
import type { OfficeSnapshot } from "../../types";

/** Alpha holds officeBuilding, Beta holds corner_shop, arcade is in no rule. */
function snapshot(rules: boolean, repos = ["p-shop", "p-office", "p-arcade"]): OfficeSnapshot {
  const demo = makeDemoSnapshot();
  const projects = demo.projects.filter((p) => [...repos, demo.setupProjectId].includes(p.id));
  const projectRules = rules
    ? [
        { id: "alpha", name: "Alpha", prefixes: [], contains: [], repos: ["officeBuilding"] },
        { id: "beta", name: "Beta", prefixes: [], contains: [], repos: ["corner_shop"] },
      ]
    : [];
  return { ...demo, projects, projectRules };
}

function show(snap: OfficeSnapshot): React.ReactElement {
  const { memory: _memory, ...grouping } = groupByProject(snap, EMPTY_MEMORY);
  return (
    <ProjectsContext value={{ ...grouping, rules: snap.projectRules, rulesError: null, similar: () => [], assignProject: () => Promise.resolve() }}>
      <BuildingView
        onOpenRequest={() => {}}
        snapshot={snap}
        mode="office"
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
    </ProjectsContext>
  );
}

const labels = (): string[] => screen.queryAllByTestId("block-label").map((l) => l.textContent ?? "");

describe("project block labels in the office", () => {
  it("puts one label per project above its floors and Ungrouped last", async () => {
    render(show(snapshot(true)));
    const plates = await screen.findAllByTestId("floor-nameplate");
    expect(plates.slice(1).map((p) => p.getAttribute("data-project"))).toEqual(["p-office", "p-shop", "p-arcade"]);
    expect(labels()).toEqual(["Alpha", "Beta", "Ungrouped"]);
  });

  it("keeps the labels out of every nameplate", async () => {
    const { container } = render(show(snapshot(true)));
    await screen.findAllByTestId("floor-nameplate");
    expect(container.querySelectorAll(".storey .nameplate [data-testid='block-label']")).toHaveLength(0);
    expect(container.querySelectorAll(".storey [data-testid='block-label']")).toHaveLength(0);
  });

  it("renders no block label when no rule names a project", async () => {
    render(show(snapshot(false)));
    await screen.findAllByTestId("floor-nameplate");
    expect(labels()).toEqual([]);
  });

  it("gives the setup floor no label and omits Ungrouped when every floor is placed", async () => {
    render(show(snapshot(true, ["p-shop", "p-office"])));
    await screen.findAllByTestId("floor-nameplate");
    expect(labels()).toEqual(["Alpha", "Beta"]);
  });
});
