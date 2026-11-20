import { render, screen, waitFor } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mockApi";
import type { FloorModel } from "../components/building/Storey";
import { ProjectsContext } from "../state/ProjectsContext";
import type { Cluster, SessionTag } from "../state/clusters";
import type { Session } from "../types";
import type { Present } from "../state/usePresence";
import { Archipelago } from "./Archipelago";
import { buildIsland } from "./build";

// jsdom has no WebGL: a renderer that draws nothing lets the scene be built, the labels and the name tags shown.
vi.mock("three", async (orig) => ({
  ...(await orig<typeof import("three")>()),
  WebGLRenderer: class {
    domElement = document.createElement("canvas");
    setPixelRatio(): void {}
    setClearColor(): void {}
    setSize(): void {}
    render(): void {}
    dispose(): void {}
    forceContextLoss(): void {}
  },
}));
vi.mock("./build", async (orig) => {
  const real = await orig<typeof import("./build")>();
  return { ...real, buildIsland: vi.fn(real.buildIsland) };
});

const cluster = (id: string | null, label: string, kind: Cluster["kind"] = "project"): Cluster => ({ id, label, kind, source: null, repoIds: [] });
const ALPHA = cluster("alpha", "Alpha");
const BETA = cluster("beta", "Beta");
const NONE = cluster(null, "Ungrouped", "ungrouped");

const floor = (id: string, name: string, c: Cluster | null, plant = false, sessions: Session[] = []): Present<FloorModel> => ({
  key: id,
  phase: "here",
  item: { project: { id, name, path: `/work/${name}`, sessionIds: [] }, sessions, light: "off", counts: { active: 0, total: 0 }, pending: 0, dimmed: false, plant, stamp: null, cluster: c },
});

async function show(entries: Present<FloorModel>[], tags: ReadonlyMap<string, SessionTag> = new Map()) {
  const snapshot = await new MockApi().getSnapshot();
  const noop = (): void => undefined;
  const props = { snapshot, onGone: noop, animate: false, onEnter: noop, onOpenSession: noop, onRenameIsland: noop, onMoveSession: noop, onAddFloor: noop, onDemolish: noop };
  const ui = (list: Present<FloorModel>[]) => (
    <ProjectsContext.Provider value={{ clusters: [], clusterOf: new Map(), tags, unplaced: [], rules: [], rulesError: null, similar: () => [], assignProject: () => Promise.resolve() }}>
      <Archipelago {...props} entries={list} />
    </ProjectsContext.Provider>
  );
  const view = render(ui(entries));
  return { snapshot, rerender: (list: Present<FloorModel>[]) => view.rerender(ui(list)) };
}

const rowNames = (): string[] => [...screen.getByRole("navigation", { name: "Islands" }).querySelectorAll(".arch-list-heading, .nameplate-name")].map((e) => e.textContent ?? "");
const builtAt = (id: string): string =>
  JSON.stringify(vi.mocked(buildIsland).mock.calls.map((c) => c[0]).filter((s) => s.projectId === id).map((s) => [s.x, s.z]).at(-1));

describe("islands in project clusters", () => {
  let setupId = "";
  beforeAll(async () => {
    setupId = (await new MockApi().getSnapshot()).setupProjectId;
  });
  const list = (): Present<FloorModel>[] => [floor("1", "repo-one", ALPHA), floor("2", "repo-two", BETA), floor("3", "solo", NONE), floor(setupId, "Setup", null, true)];

  it("lists a heading per project with its islands, Ungrouped last, the lighthouse after, no Waiting heading", async () => {
    await show(list());
    expect(rowNames()).toEqual(["Alpha", "repo-one", "Beta", "repo-two", "Ungrouped", "solo", "Setup"]);
    expect(screen.queryByText(/waiting on you/i)).toBeNull();
  });

  it("puts a project label over each cluster, and none for an empty one", async () => {
    await show(list().slice(0, 2));
    expect(await screen.findAllByText(/^(Alpha|Beta)$/, { selector: ".arch-cluster" })).toHaveLength(2);
    expect(screen.queryByText("Ungrouped", { selector: ".arch-cluster" })).toBeNull();
  });

  it("moves a repo's island and list row when it changes cluster, and no other island", async () => {
    const { rerender } = await show([floor("1", "repo-one", ALPHA), floor("2", "repo-two", NONE), floor("3", "repo-three", NONE)]);
    await screen.findAllByText("Alpha", { selector: ".arch-cluster" });
    const before = ["1", "3"].map(builtAt);
    const at2 = builtAt("2");
    rerender([floor("1", "repo-one", ALPHA), floor("2", "repo-two", ALPHA), floor("3", "repo-three", NONE)]);
    await waitFor(() => expect(builtAt("2")).not.toBe(at2));
    expect(["1", "3"].map(builtAt)).toEqual(before);
    expect(rowNames()).toEqual(["Alpha", "repo-one", "repo-two", "Ungrouped", "repo-three"]);
  });

  it("adds the session's project tag to its name tag", async () => {
    const snapshot = await new MockApi().getSnapshot();
    const session = snapshot.sessions[0];
    if (session === undefined) throw new Error("seed has no session");
    const tag: SessionTag = { sessionId: session.id, kind: "project", label: "Beta", home: null };
    await show([floor("1", "repo-one", ALPHA, false, [session])], new Map([[session.id, tag]]));
    const el = await waitFor(() => {
      const t = document.querySelector(".arch-tag");
      if (t === null) throw new Error("no tag yet");
      return t;
    });
    expect(el.textContent).toMatch(/Beta/);
  });
  const tagOf = async (tag: SessionTag): Promise<string> => {
    const snapshot = await new MockApi().getSnapshot();
    const session = snapshot.sessions[0];
    if (session === undefined) throw new Error("seed has no session");
    await show([floor("1", "repo-one", ALPHA, false, [session])], new Map([[session.id, { ...tag, sessionId: session.id }]]));
    const el = await waitFor(() => {
      const t = document.querySelector(".arch-tag");
      if (t === null) throw new Error("no tag yet");
      return t;
    });
    return el.textContent ?? "";
  };

  it("leaves the unplaced marker out of a hut tag", async () => {
    expect(await tagOf({ sessionId: "", kind: "unplaced", label: "Which project?", home: null })).not.toContain("Which project?");
  });

  it("shows a harness session's home in its hut tag", async () => {
    expect(await tagOf({ sessionId: "", kind: "harness", label: "Omega", home: "Beta" })).toContain("Omega \u00b7 Beta");
  });
});
