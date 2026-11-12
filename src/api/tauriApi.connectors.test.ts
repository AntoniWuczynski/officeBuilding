import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FloorLink } from "../types";
import { makeDemoSnapshot, makeSeedSnapshot } from "./mockData";
import { makeSettings } from "./mockSettings";
import { parseWire, settingsSchema, snapshotSchema } from "./wire";

const invoke = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => undefined) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => undefined) }) }));

const { TauriApi } = await import("./tauriApi");

const LINK: FloorLink = { kind: "linear", teamId: "t", teamKey: "ENG", teamName: "Engineering", project: null, label: "bug" };

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
});

describe("TauriApi connectors", () => {
  it("calls each command with its arguments", async () => {
    const api = new TauriApi();
    await api.connectFloor("p-1", LINK);
    await api.disconnectFloor("p-1", "linear");
    await api.recheckConnectors();
    await api.openTodoLink("p-1|linear|x");
    expect(invoke.mock.calls).toEqual([
      ["connect_floor", { projectId: "p-1", link: LINK }],
      ["disconnect_floor", { projectId: "p-1", connector: "linear" }],
      ["recheck_connectors", undefined],
      ["open_todo_link", { todoId: "p-1|linear|x" }],
    ]);
  });

  it("parses the key status, for a read and a write", async () => {
    const api = new TauriApi();
    invoke.mockResolvedValueOnce({ kind: "saved", account: "Demo User" });
    await expect(api.linearKeyStatus()).resolves.toEqual({ kind: "saved", account: "Demo User" });
    invoke.mockResolvedValueOnce({ kind: "missing" });
    await expect(api.setLinearKey(null)).resolves.toEqual({ kind: "missing" });
    expect(invoke).toHaveBeenLastCalledWith("set_linear_key", { key: null });
    invoke.mockResolvedValueOnce({ kind: "unavailable" });
    await expect(api.linearKeyStatus()).rejects.toThrow(/unexpected Linear key status/);
  });

  it("parses the team list", async () => {
    const api = new TauriApi();
    const teams = [{ id: "t", key: "ENG", name: "Engineering", projects: [{ id: "pr", name: "Checkout" }] }];
    invoke.mockResolvedValueOnce(teams);
    await expect(api.linearTeams()).resolves.toEqual(teams);
    invoke.mockResolvedValueOnce([{ id: "t" }]);
    await expect(api.linearTeams()).rejects.toThrow(/unexpected list of Linear teams/);
  });
});

describe("wire, connectors", () => {
  it("parses a snapshot with a Linear todo and a connection", () => {
    const demo = makeDemoSnapshot();
    expect(parseWire(snapshotSchema, "snapshot", demo)).toEqual(demo);
    expect(demo.connections).toHaveLength(1);
  });

  it("rejects a todo that still has sourceFile and no source", () => {
    const seed = makeSeedSnapshot();
    const [first, ...rest] = seed.todos;
    if (first === undefined) throw new Error("no todo in the seed");
    const { source: _source, ...bare } = first;
    const old = { ...bare, sourceFile: null };
    expect(() => parseWire(snapshotSchema, "snapshot", { ...seed, todos: [old, ...rest] })).toThrow(/todos\.0\.source/);
  });

  it("rejects settings without connectors", () => {
    const { connectors: _connectors, ...without } = makeSettings();
    expect(() => parseWire(settingsSchema, "settings", without)).toThrow(/unexpected settings at connectors/);
  });
});
