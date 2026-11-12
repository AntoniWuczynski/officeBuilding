import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Connection, TodoItem } from "../../types";
import { ConnectorsContext } from "../connectors/ConnectorsContext";
import type { ConnectorActions } from "../connectors/ConnectorsContext";
import { TodoList } from "./TodoList";

const item = (id: string, source: TodoItem["source"]): TodoItem => ({ id, projectId: "p", text: `text ${id}`, done: false, assignee: "ai", source });

const NOW = new Date("2026-09-28T15:41:00Z");
const connection = (over: Partial<Connection> = {}): Connection => ({
  projectId: "p",
  link: { kind: "linear", teamId: "t", teamKey: "ENG", teamName: "Engineering", project: null, label: null },
  fetchedAt: "2026-09-28T15:40:00Z",
  error: null,
  needsKey: false,
  truncated: false,
  ...over,
});

function actionsMock(): ConnectorActions {
  return {
    keyStatus: vi.fn(),
    setKey: vi.fn(),
    teams: vi.fn(),
    connect: vi.fn(),
    disconnect: vi.fn(),
    recheck: vi.fn(),
    openLink: vi.fn(),
    openSettings: vi.fn(),
  };
}

function show(items: readonly TodoItem[], opts: { actions?: ConnectorActions; connection?: Connection; onTick?: (id: string) => void } = {}): void {
  const list = <TodoList items={items} empty="Nothing." hint="Plain hint." connection={opts.connection} now={NOW} onTick={opts.onTick ?? vi.fn()} />;
  render(opts.actions === undefined ? list : <ConnectorsContext value={opts.actions}>{list}</ConnectorsContext>);
}

const linear = item("b", { kind: "linear", identifier: "ENG-101", url: "https://linear.app/x/ENG-101" });

describe("TodoList sources", () => {
  it("badges a file item and a Linear item, and leaves a store item bare", () => {
    show([item("a", { kind: "file", file: "TODO.md" }), item("b", { kind: "linear", identifier: "ENG-101", url: "https://linear.app/x" }), item("c", { kind: "app" })]);
    const badges = screen.getAllByTestId("todo-source").map((b) => b.textContent);
    expect(badges).toEqual(["TODO.md", "Linear · ENG-101"]);
  });

  it("names every file once in the hint", () => {
    show([item("a", { kind: "file", file: "TODO.md" }), item("b", { kind: "file", file: "HUMAN_TODO.md" }), item("c", { kind: "file", file: "TODO.md" })]);
    expect(screen.getByText("From TODO.md and HUMAN_TODO.md. Ticking an item writes [x] into the file.")).toBeInTheDocument();
  });

  it("keeps the file sentence word for word when one file feeds the list", () => {
    show([item("a", { kind: "file", file: "TODO.md" })]);
    expect(screen.getByText("From TODO.md. Ticking an item writes [x] into the file.")).toBeInTheDocument();
  });

  it("falls back to the hint without a file", () => {
    show([item("a", { kind: "linear", identifier: "ENG-1", url: "https://linear.app/x" })]);
    expect(screen.getByText("Plain hint.")).toBeInTheDocument();
  });
});

describe("TodoList and Linear", () => {
  it("makes the Linear badge a button that opens the issue without ticking", async () => {
    const actions = actionsMock();
    const onTick = vi.fn();
    show([linear], { actions, onTick });
    await userEvent.click(screen.getByRole("button", { name: "Open ENG-101 in Linear" }));
    expect(actions.openLink).toHaveBeenCalledWith("b");
    expect(screen.getByTestId("todo-check")).not.toBeChecked();
    expect(onTick).not.toHaveBeenCalled();
  });

  it("asks before ticking a Linear item, and ticks on Mark as Done", async () => {
    const onTick = vi.fn();
    show([linear], { actions: actionsMock(), onTick });
    await userEvent.click(screen.getByTestId("todo-check"));
    expect(screen.getByText("Mark ENG-101 as Done in Linear?")).toBeInTheDocument();
    expect(onTick).not.toHaveBeenCalled();
    await userEvent.click(screen.getByTestId("todo-confirm"));
    expect(onTick).toHaveBeenCalledWith("b");
    expect(screen.queryByTestId("todo-confirm")).not.toBeInTheDocument();
  });

  it("drops the question on Cancel and on Escape", async () => {
    const onTick = vi.fn();
    show([linear], { actions: actionsMock(), onTick });
    await userEvent.click(screen.getByTestId("todo-check"));
    await userEvent.click(screen.getByTestId("todo-cancel"));
    expect(screen.queryByTestId("todo-confirm")).not.toBeInTheDocument();
    await userEvent.click(screen.getByTestId("todo-check"));
    fireEvent.keyDown(screen.getByTestId("todo-confirm"), { key: "Escape" });
    expect(screen.queryByTestId("todo-confirm")).not.toBeInTheDocument();
    expect(onTick).not.toHaveBeenCalled();
  });

  it("ticks a file item at once", async () => {
    const onTick = vi.fn();
    show([item("a", { kind: "file", file: "TODO.md" })], { onTick });
    await userEvent.click(screen.getByTestId("todo-check"));
    expect(onTick).toHaveBeenCalledWith("a");
  });

  it("shows the connection line with Re-check, or Open Settings when a key is needed", async () => {
    const actions = actionsMock();
    show([linear], { actions, connection: connection() });
    expect(screen.getByTestId("connector-line")).toHaveTextContent("Linear, checked");
    await userEvent.click(screen.getByTestId("connector-recheck"));
    expect(actions.recheck).toHaveBeenCalled();
  });

  it("offers Open Settings when the connection needs a key", async () => {
    const actions = actionsMock();
    show([], { actions, connection: connection({ needsKey: true, fetchedAt: null }) });
    expect(screen.queryByTestId("connector-recheck")).not.toBeInTheDocument();
    await userEvent.click(screen.getByTestId("connector-settings"));
    expect(actions.openSettings).toHaveBeenCalled();
  });

  it("still shows the line on a linked floor with no items", () => {
    show([], { actions: actionsMock(), connection: connection() });
    expect(screen.getByTestId("connector-line")).toBeInTheDocument();
    expect(screen.getByText("Nothing.")).toBeInTheDocument();
  });

  it("renders no buttons and a plain badge without a provider", () => {
    show([linear], { connection: connection() });
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(screen.getByTestId("todo-source").tagName).toBe("SPAN");
    expect(screen.getByTestId("connector-line")).toBeInTheDocument();
  });
});
