import { describe, expect, it } from "vitest";
import { createEvent, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";

async function enter(projectId: string): Promise<void> {
  const user = userEvent.setup();
  await screen.findAllByTestId("storey");
  const storey = screen.getAllByTestId("storey").find((s) => s.getAttribute("data-project") === projectId);
  if (storey === undefined) throw new Error(`no storey ${projectId}`);
  await user.click(within(storey).getByTestId("floor-nameplate"));
  await screen.findByTestId("back-button");
}

function workerOf(sessionId: string): HTMLElement {
  const el = screen.getAllByTestId("worker").find((w) => w.getAttribute("data-session") === sessionId);
  if (el === undefined) throw new Error(`no worker ${sessionId}`);
  return el;
}

/** Fires a contextmenu on `el` and reports whether the browser's own menu was left alone. */
function rightClick(el: HTMLElement): boolean {
  return fireEvent.contextMenu(el, { clientX: 120, clientY: 80 });
}

async function shopFloor(api: MockApi = new MockApi()): Promise<MockApi> {
  render(<App api={api} />);
  await enter("p-shop");
  await waitFor(() => workerOf("s-2"));
  // The menu waits for the editor list, so the labels are final.
  await waitFor(() => expect(api.opened).toEqual([]));
  return api;
}

describe("the right-click menu", () => {
  it("opens on a right-click with the caption and the installed editors, and stops the browser's menu", async () => {
    await shopFloor();
    const worker = workerOf("s-2");
    const e = createEvent.contextMenu(worker, { clientX: 120, clientY: 80 });
    fireEvent(worker, e);
    expect(e.defaultPrevented).toBe(true);
    const menu = await screen.findByTestId("editor-menu");
    expect(within(menu).getByText("Opens its worktree on branch fix/orders-ingest")).toBeInTheDocument();
    const lines = within(menu).getAllByTestId("editor-menu-item");
    expect(lines.length).toBeGreaterThan(0);
    expect(lines[0]?.textContent).toMatch(/^Open in /);
    expect(worker).toHaveAttribute("aria-keyshortcuts", "Meta+Shift+O");
  });

  it("says its floor's folder for a desk outside a worktree", async () => {
    await shopFloor();
    rightClick(workerOf("s-1"));
    expect(await screen.findByText("Opens its floor's folder")).toBeInTheDocument();
  });

  it("opens the desk's place in the picked editor and closes", async () => {
    const user = userEvent.setup();
    const api = await shopFloor();
    rightClick(workerOf("s-2"));
    const lines = await screen.findAllByTestId("editor-menu-item");
    const line = lines[0];
    if (line === undefined) throw new Error("no lines");
    const editor = line.getAttribute("data-editor");
    await user.click(line);
    await waitFor(() => expect(api.opened).toHaveLength(1));
    expect(api.opened[0]).toEqual({ place: { kind: "session", sessionId: "s-2" }, editor });
    expect(screen.queryByTestId("editor-menu")).not.toBeInTheDocument();
  });

  it("closes on Escape and on a press elsewhere", async () => {
    await shopFloor();
    rightClick(workerOf("s-2"));
    const first = (await screen.findAllByTestId("editor-menu-item"))[0];
    fireEvent.keyDown(first ?? document.body, { key: "Escape" });
    expect(screen.queryByTestId("editor-menu")).not.toBeInTheDocument();
    rightClick(workerOf("s-2"));
    await screen.findByTestId("editor-menu");
    fireEvent.pointerDown(document.body);
    expect(screen.queryByTestId("editor-menu")).not.toBeInTheDocument();
  });

  it("hides the hover card and names the worktree and the hint in it", async () => {
    await shopFloor();
    const worker = workerOf("s-2");
    fireEvent.mouseEnter(worker);
    const card = await screen.findByTestId("worker-card");
    expect(within(card).getByText("Worktree on fix/orders-ingest")).toBeInTheDocument();
    expect(within(card).getByText(/^Right-click to open in /)).toBeInTheDocument();
    rightClick(worker);
    expect(screen.queryByTestId("worker-card")).not.toBeInTheDocument();
  });

  it("gives the card no hint when nothing is installed", async () => {
    const api = new MockApi();
    api.setInstalledEditors([]);
    await shopFloor(api);
    fireEvent.mouseEnter(workerOf("s-2"));
    const card = await screen.findByTestId("worker-card");
    expect(within(card).getByText("Worktree on fix/orders-ingest")).toBeInTheDocument();
    expect(within(card).queryByText(/Right-click/)).not.toBeInTheDocument();
  });

  it("puts the branch in the worker's label", async () => {
    await shopFloor();
    expect(workerOf("s-2").getAttribute("aria-label")).toContain("in a worktree on branch fix/orders-ingest");
  });
});

describe("a helper's right-click menu", () => {
  it("makes only a worktree helper's figure a button", async () => {
    await shopFloor();
    const buttons = screen.getAllByTestId("pod-worker");
    expect(buttons.map((b) => b.getAttribute("data-helper"))).toEqual(["s-1-h1"]);
    expect(buttons[0]).toHaveAttribute("aria-label", "Helper in a worktree on branch helper-1. Open its code in an editor.");
    expect(screen.getAllByTestId("pod-helper").length).toBeGreaterThan(1);
  });

  it("makes no helper a button in a pod too small to seat people", async () => {
    render(<App api={new MockApi()} />);
    await enter("p-office");
    await waitFor(() => workerOf("s-4"));
    expect(screen.queryByTestId("pod-worker")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pod-helper")).not.toBeInTheDocument();
  });

  it("opens the menu on a click and opens the helper place on a pick", async () => {
    const user = userEvent.setup();
    const api = await shopFloor();
    const button = screen.getAllByTestId("pod-worker")[0];
    if (button === undefined) throw new Error("no helper button");
    fireEvent.click(button, { clientX: 50, clientY: 60 });
    expect(await screen.findByText("Opens its worktree on branch helper-1")).toBeInTheDocument();
    const line = (await screen.findAllByTestId("editor-menu-item"))[0];
    if (line === undefined) throw new Error("no lines");
    await user.click(line);
    await waitFor(() => expect(api.opened).toHaveLength(1));
    expect(api.opened[0]?.place).toEqual({ kind: "helper", sessionId: "s-1", helperId: "s-1-h1" });
  });

  it("opens the menu on a right-click too", async () => {
    await shopFloor();
    const button = screen.getAllByTestId("pod-worker")[0];
    if (button === undefined) throw new Error("no helper button");
    const e = createEvent.contextMenu(button, { clientX: 50, clientY: 60 });
    fireEvent(button, e);
    expect(e.defaultPrevented).toBe(true);
    expect(await screen.findByTestId("editor-menu")).toBeInTheDocument();
  });
});
