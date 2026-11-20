import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mockApi";
import type { FloorModel } from "../components/building/Storey";
import type { Present } from "../state/usePresence";
import { Archipelago } from "./Archipelago";
import { buildIsland } from "./build";

// jsdom has no WebGL: a renderer that draws nothing lets the scene be built and the labels shown.
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

const floor = (id: string, name: string): FloorModel => ({
  project: { id, name, path: `/work/${id}`, sessionIds: [] },
  sessions: [],
  light: "off",
  counts: { active: 0, total: 0 },
  pending: 0,
  dimmed: false,
  plant: false,
  stamp: null,
  cluster: null,
});
const entries = (a: string, b: string): Present<FloorModel>[] => [
  { key: "1", item: floor("1", a), phase: "here" },
  { key: "2", item: floor("2", b), phase: "here" },
];

describe("renaming an island", () => {
  it("changes the label and leaves the built scene alone", async () => {
    const snapshot = await new MockApi().getSnapshot();
    const noop = (): void => undefined;
    const props = { snapshot, onGone: noop, name: "Map", animate: false, onRename: noop, onEnter: noop, onOpenSession: noop, onRenameIsland: noop, onMoveSession: noop, onAddFloor: noop, onDemolish: noop };
    const { rerender } = render(<Archipelago {...props} entries={entries("alpha", "beta")} />);
    expect(await screen.findByText("alpha", { selector: ".arch-label b" })).toBeInTheDocument();
    const built = vi.mocked(buildIsland).mock.calls.length;
    expect(built).toBeGreaterThanOrEqual(2);

    rerender(<Archipelago {...props} entries={entries("gamma", "beta")} />);
    expect(await screen.findByText("gamma", { selector: ".arch-label b" })).toBeInTheDocument();
    expect(screen.queryByText("alpha", { selector: ".arch-label b" })).toBeNull();
    expect(vi.mocked(buildIsland).mock.calls.length).toBe(built);
  });
});
