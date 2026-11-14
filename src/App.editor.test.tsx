import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

/** Enter p-arcade, hire a structured desk (its panel needs no real terminal) and wait for it to sit down. */
async function seatHire(api: MockApi): Promise<{ id: string; worker: () => HTMLElement }> {
  render(<App api={api} />);
  await enter("p-arcade");
  const hired = await api.spawnSession("p-arcade", { tool: "codex", model: "gpt-5.6-terra", effort: "medium", title: "watch me", ultracode: false, view: "chat" });
  fireEvent.animationEnd(await screen.findByTestId("walk"));
  const worker = (): HTMLElement => {
    const el = screen.getAllByTestId("worker").find((w) => w.getAttribute("data-session") === hired.id);
    if (el === undefined) throw new Error("not seated yet");
    return el;
  };
  await waitFor(() => worker());
  return { id: hired.id, worker };
}

const cmdShiftO = (init: KeyboardEventInit = {}): boolean => fireEvent.keyDown(window, { key: "o", metaKey: true, shiftKey: true, ...init });

describe("Open in editor", () => {
  it("opens a desk from its panel header and names the editor in the toast", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const { id, worker } = await seatHire(api);
    await user.click(worker());
    const panel = await screen.findByTestId("terminal-panel");
    const main = await within(panel).findByRole("button", { name: "Open in Cursor" });
    await user.click(main);
    await waitFor(() => expect(api.opened).toEqual([{ place: { kind: "session", sessionId: id }, editor: "cursor" }]));
    expect(await screen.findByText("Opened “watch me” in Cursor.")).toBeInTheDocument();
  });

  it("opens the open desk with Cmd+Shift+O", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const { id, worker } = await seatHire(api);
    await user.click(worker());
    await screen.findByTestId("terminal-panel");
    expect(cmdShiftO()).toBe(false);
    await waitFor(() => expect(api.opened).toHaveLength(1));
    expect(api.opened[0]?.place).toEqual({ kind: "session", sessionId: id });
    cmdShiftO({ repeat: true });
    expect(api.opened).toHaveLength(1);
  });

  it("opens the keyboard-focused worker when no panel is open", async () => {
    const api = new MockApi();
    const { id, worker } = await seatHire(api);
    worker().focus();
    expect(cmdShiftO()).toBe(false);
    await waitFor(() => expect(api.opened).toHaveLength(1));
    expect(api.opened[0]?.place).toEqual({ kind: "session", sessionId: id });
  });

  it("leaves the key alone with neither a panel nor a focused worker", async () => {
    const api = new MockApi();
    await seatHire(api);
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    expect(cmdShiftO()).toBe(true);
    expect(api.opened).toEqual([]);
  });

  it("opens a signed-out session from its row", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    render(<App api={api} />);
    await enter("p-shop");
    await user.click(screen.getByTestId("rail-signed-out"));
    const row = screen.getAllByTestId("signed-out-row")[0] ?? document.body;
    await user.click(await within(row).findByRole("button", { name: "Open in Cursor" }));
    await waitFor(() => expect(api.opened).toEqual([{ place: { kind: "session", sessionId: "e-1" }, editor: "cursor" }]));
    expect(await screen.findByText("Opened “refund emails” in Cursor.")).toBeInTheDocument();
  });

  it("opens the floor's folder from the Info panel", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    render(<App api={api} />);
    await enter("p-shop");
    await user.click(screen.getByTestId("rail-info"));
    const sidebar = screen.getByTestId("sidebar");
    await user.click(await within(sidebar).findByRole("button", { name: "Open in Cursor" }));
    await waitFor(() => expect(api.opened).toEqual([{ place: { kind: "floor", projectId: "p-shop" }, editor: "cursor" }]));
    expect(await screen.findByText(/^Opened the .+ floor in Cursor\.$/)).toBeInTheDocument();
  });

  it("shows an error toast for a lost folder and reloads the editor list", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const calls = { n: 0 };
    const original = api.editors.bind(api);
    api.editors = () => {
      calls.n += 1;
      return original();
    };
    render(<App api={api} />);
    await enter("p-shop");
    api.loseFolder("e-1");
    await user.click(screen.getByTestId("rail-signed-out"));
    const row = screen.getAllByTestId("signed-out-row")[0] ?? document.body;
    await waitFor(() => expect(calls.n).toBeGreaterThan(0));
    const before = calls.n;
    await user.click(await within(row).findByRole("button", { name: "Open in Cursor" }));
    expect(await screen.findByText("Could not open “refund emails” in Cursor: that worktree is gone.")).toBeInTheDocument();
    await waitFor(() => expect(calls.n).toBeGreaterThan(before));
  });
});
