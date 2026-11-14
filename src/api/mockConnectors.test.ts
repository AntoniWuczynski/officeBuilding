import { describe, expect, it } from "vitest";
import type { FloorLink, OfficeSnapshot } from "../types";
import { MockApi } from "./mockApi";
import { MOCK_TEAMS, mockLinearTodos } from "./mockConnectors";
import { makeDemoSnapshot, makeSeedSnapshot } from "./mockData";
import { makeSettings } from "./mockSettings";
import { parseWire, snapshotSchema } from "./wire";

const LINK: FloorLink = { kind: "linear", teamId: "team-eng", teamKey: "ENG", teamName: "Engineering", project: null, label: null };
const NOW = new Date("2026-11-12T10:00:00Z");

const linearTodos = (s: OfficeSnapshot, projectId: string): string[] => s.todos.filter((t) => t.projectId === projectId && t.source.kind === "linear").map((t) => t.id);

describe("mockLinearTodos", () => {
  it("makes one issue for the human and one for the agents", () => {
    const [human, ai] = mockLinearTodos("p-1", LINK);
    expect(human).toMatchObject({ id: "p-1|linear|issue-1", text: "Check the refund email copy", assignee: "human", source: { kind: "linear", identifier: "ENG-101", url: "https://linear.app/example/issue/ENG-101" } });
    expect(ai).toMatchObject({ id: "p-1|linear|issue-2", text: "Add a retry to the orders ingest", assignee: "ai" });
  });
  it("offers two teams", () => {
    expect(MOCK_TEAMS.map((t) => t.key)).toEqual(["ENG", "OPS"]);
  });
});

describe("MockApi connectors", () => {
  it("starts without a key on the plain seed and with one on the demo", async () => {
    await expect(new MockApi().linearKeyStatus()).resolves.toEqual({ kind: "missing" });
    await expect(new MockApi(makeDemoSnapshot()).linearKeyStatus()).resolves.toEqual({ kind: "saved", account: null });
    const demo = makeDemoSnapshot();
    expect(parseWire(snapshotSchema, "snapshot", demo)).toEqual(demo);
    expect(demo.connections[0]).toMatchObject({ projectId: "p-shop", fetchedAt: "2026-09-28T15:40:00Z" });
  });

  it("checks a key before keeping it", async () => {
    const api = new MockApi();
    await expect(api.setLinearKey("  ")).rejects.toThrow("that does not look like a Linear API key");
    await expect(api.setLinearKey("a bad key")).rejects.toThrow("Linear did not accept that key");
    await expect(api.linearKeyStatus()).resolves.toEqual({ kind: "missing" });
    await expect(api.setLinearKey("lin_api_demo")).resolves.toEqual({ kind: "saved", account: "Demo User" });
  });

  it("removes the key, the Linear items and the freshness", async () => {
    const api = new MockApi(makeDemoSnapshot());
    await expect(api.setLinearKey(null)).resolves.toEqual({ kind: "missing" });
    const s = await api.getSnapshot();
    expect(linearTodos(s, "p-shop")).toEqual([]);
    expect(s.connections[0]).toMatchObject({ needsKey: true, fetchedAt: null, error: "no Linear API key is saved" });
  });

  it("brings the items back when a key is saved again", async () => {
    const api = new MockApi(makeDemoSnapshot(), () => NOW);
    await api.setLinearKey(null);
    await api.setLinearKey("lin_api_demo");
    const s = await api.getSnapshot();
    expect(linearTodos(s, "p-shop")).toHaveLength(2);
    expect(s.connections[0]).toMatchObject({ needsKey: false, error: null, fetchedAt: NOW.toISOString() });
  });

  it("lists teams only with a key", async () => {
    await expect(new MockApi().linearTeams()).rejects.toThrow("no Linear API key is saved");
    await expect(new MockApi(makeDemoSnapshot()).linearTeams()).resolves.toEqual(MOCK_TEAMS);
  });

  it("connects a floor with a key, then replaces the link", async () => {
    const api = new MockApi(makeDemoSnapshot(), () => NOW);
    await api.connectFloor("p-arcade", LINK);
    let s = await api.getSnapshot();
    expect(linearTodos(s, "p-arcade")).toHaveLength(2);
    expect(s.connections.find((c) => c.projectId === "p-arcade")).toMatchObject({ fetchedAt: NOW.toISOString(), needsKey: false });
    await api.connectFloor("p-arcade", { ...LINK, label: "bug" });
    s = await api.getSnapshot();
    expect(s.connections.filter((c) => c.projectId === "p-arcade")).toHaveLength(1);
    expect(linearTodos(s, "p-arcade")).toHaveLength(2);
  });

  it("connects a floor without a key and shows no items", async () => {
    const api = new MockApi();
    await api.connectFloor("p-shop", LINK);
    const s = await api.getSnapshot();
    expect(linearTodos(s, "p-shop")).toEqual([]);
    expect(s.connections[0]).toMatchObject({ needsKey: true, fetchedAt: null });
  });

  it("refuses an unknown floor", async () => {
    await expect(new MockApi().connectFloor("p-nope", LINK)).rejects.toThrow("p-nope is not a floor of this building");
  });

  it("disconnects a floor and takes its Linear items off, keeping the rest", async () => {
    const api = new MockApi(makeDemoSnapshot());
    const before = (await api.getSnapshot()).todos.filter((t) => t.source.kind !== "linear").length;
    await api.disconnectFloor("p-shop", "linear");
    const s = await api.getSnapshot();
    expect(s.connections).toEqual([]);
    expect(s.todos).toHaveLength(before);
  });

  it("re-checks with a key and does nothing without one", async () => {
    const api = new MockApi(makeDemoSnapshot(), () => NOW);
    api.setConnectionError("p-shop", "Linear is unreachable");
    expect((await api.getSnapshot()).connections[0]).toMatchObject({ error: "Linear is unreachable", fetchedAt: "2026-09-28T15:40:00Z" });
    await api.recheckConnectors();
    expect((await api.getSnapshot()).connections[0]).toMatchObject({ error: null, fetchedAt: NOW.toISOString() });
    const bare = new MockApi();
    await bare.recheckConnectors();
    expect((await bare.getSnapshot()).connections).toEqual([]);
  });

  it("opens a Linear item's page and refuses any other", async () => {
    const api = new MockApi(makeDemoSnapshot());
    await api.openTodoLink("p-shop|linear|issue-1");
    expect(api.openedLinks).toEqual(["p-shop|linear|issue-1"]);
    await expect(api.openTodoLink("t-5")).rejects.toThrow("that item has no page to open");
    await expect(api.openTodoLink("nope")).rejects.toThrow("that item has no page to open");
  });

  it("plays a tick failure once and leaves the item open", async () => {
    const api = new MockApi(makeDemoSnapshot());
    api.failNextLinearTick("Linear refused");
    await expect(api.tickTodo("p-shop|linear|issue-1")).rejects.toThrow("Linear refused");
    expect((await api.getSnapshot()).todos.find((t) => t.id === "p-shop|linear|issue-1")?.done).toBe(false);
    await api.tickTodo("p-shop|linear|issue-1");
    expect((await api.getSnapshot()).todos.find((t) => t.id === "p-shop|linear|issue-1")?.done).toBe(true);
  });

  it("does not play a tick failure on a non-Linear item", async () => {
    const api = new MockApi(makeDemoSnapshot());
    api.failNextLinearTick("Linear refused");
    await api.tickTodo("t-5");
    expect((await api.getSnapshot()).todos.find((t) => t.id === "t-5")?.done).toBe(true);
  });

  it("drops a demolished floor's connection", async () => {
    const api = new MockApi(makeDemoSnapshot());
    await api.demolishFloor("p-shop");
    expect((await api.getSnapshot()).connections).toEqual([]);
  });

  it("checks the agent label", async () => {
    const api = new MockApi(makeSeedSnapshot());
    const label = (linearAgentLabel: string) => api.updateSettings({ connectors: { linearAgentLabel } });
    await expect(label("   ")).rejects.toThrow("the agent label must be 1 to 80 characters");
    await expect(label("x".repeat(81))).rejects.toThrow("the agent label must be 1 to 80 characters");
    await expect(label("robots")).resolves.toMatchObject({ connectors: { linearAgentLabel: "robots" } });
    expect(makeSettings().connectors.linearAgentLabel).toBe("agent");
  });
});
