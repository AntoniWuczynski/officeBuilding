import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";
import { makeDemoSnapshot, makeSeedSnapshot } from "./api/mockData";

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
  await enter("p-arcade");
  const hired = await api.spawnSession("p-arcade", { tool: "codex", model: "gpt-5.6-terra", effort: "medium", title: "watch me", ultracode: false, view: "chat" });
  // The new hire walks in from the front, then takes the desk.
  fireEvent.animationEnd(await screen.findByTestId("walk"));
  const worker = (): HTMLElement => {
    const el = screen.getAllByTestId("worker").find((w) => w.getAttribute("data-session") === hired.id);
    if (el === undefined) throw new Error("not seated yet");
    return el;
  };
  return { id: hired.id, worker };
}

const cmdComma = (init: KeyboardEventInit = {}): boolean => fireEvent.keyDown(window, { key: ",", metaKey: true, ...init });

describe("Settings", () => {
  it("opens and closes with Cmd+, on the building view", async () => {
    render(<App api={new MockApi()} />);
    await screen.findAllByTestId("storey");
    cmdComma();
    expect(await screen.findByTestId("settings-dialog")).toBeInTheDocument();
    cmdComma({ repeat: true });
    expect(screen.getByTestId("settings-dialog")).toBeInTheDocument();
    cmdComma();
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
  });

  it("ignores a comma without Cmd", async () => {
    render(<App api={new MockApi()} />);
    await screen.findAllByTestId("storey");
    fireEvent.keyDown(window, { key: "," });
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
  });

  it("takes the place of a sidebar section on a floor, and a section takes its place back", async () => {
    const user = userEvent.setup();
    render(<App api={new MockApi()} />);
    await enter("p-shop");
    await user.click(screen.getByTestId("rail-info"));
    expect(screen.getByTestId("sidebar")).toBeInTheDocument();
    cmdComma();
    expect(await screen.findByTestId("settings-dialog")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByTestId("sidebar")).not.toBeInTheDocument());
    await user.click(screen.getByTestId("rail-info"));
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("sidebar")).toBeInTheDocument();
  });

  it("reloads the editor list when Settings opens and after the Editor group is saved", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const editors = vi.spyOn(api, "editors");
    render(<App api={api} />);
    await screen.findAllByTestId("storey");
    await waitFor(() => expect(editors).toHaveBeenCalledTimes(1));
    cmdComma();
    await screen.findByTestId("settings-dialog");
    await waitFor(() => expect(editors).toHaveBeenCalledTimes(2));
    await user.click(await screen.findByRole("switch", { name: "Sound" }));
    await waitFor(async () => expect((await api.getSettings()).notifications.sound).toBe(false));
    expect(editors).toHaveBeenCalledTimes(2);
    await user.selectOptions(await screen.findByTestId("settings-editor"), "VS Code");
    await waitFor(() => expect(editors).toHaveBeenCalledTimes(3));
    expect((await api.getSettings()).editor.preferred).toBe("vs-code");
    expect(screen.getByTestId("settings-editor")).toHaveValue("vs-code");
  });

  it("opens from the fuse box on the Setup floor only", async () => {
    const user = userEvent.setup();
    const demo = makeDemoSnapshot();
    render(<App api={new MockApi(demo)} />);
    await enter(demo.setupProjectId);
    await user.click(screen.getByTestId("object-settings"));
    expect(await screen.findByTestId("settings-dialog")).toBeInTheDocument();
    await user.click(screen.getByTestId("settings-close"));
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
    await user.click(screen.getByTestId("back-button"));
    await enter("p-shop");
    expect(screen.queryByTestId("object-settings")).not.toBeInTheDocument();
  });

  it("stays shut while the Hire dialog is open", async () => {
    const user = userEvent.setup();
    render(<App api={new MockApi()} />);
    await enter("p-shop");
    await user.click(screen.getByTestId("hire-desk"));
    await screen.findByTestId("hire-dialog");
    cmdComma();
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
  });

  it("gives way to a Hire dialog opened while it is open", async () => {
    const user = userEvent.setup();
    render(<App api={new MockApi()} />);
    await enter("p-shop");
    cmdComma();
    await screen.findByTestId("settings-dialog");
    await user.click(screen.getByTestId("hire-desk"));
    expect(await screen.findByTestId("hire-dialog")).toBeInTheDocument();
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
  });

  it("gives way to a Demolish dialog opened while it is open", async () => {
    const user = userEvent.setup();
    render(<App api={new MockApi()} />);
    await screen.findAllByTestId("storey");
    cmdComma();
    await screen.findByTestId("settings-dialog");
    const storey = screen.getAllByTestId("storey").find((s) => s.getAttribute("data-project") === "p-shop");
    if (storey === undefined) throw new Error("no storey p-shop");
    await user.click(within(storey).getByTestId("demolish-floor"));
    expect(screen.getByTestId("demolish-dialog")).toBeInTheDocument();
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
  });

  it("never shares the floor with the docked terminal", async () => {
    const user = userEvent.setup();
    const api = new MockApi(makeSeedSnapshot());
    render(<App api={api} />);
    const { worker } = await seatHire(api);
    await user.click(worker());
    expect(await screen.findByTestId("terminal-panel")).toBeInTheDocument();
    cmdComma();
    expect(await screen.findByTestId("settings-dialog")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByTestId("terminal-panel")).not.toBeInTheDocument());
    await user.click(worker());
    expect(await screen.findByTestId("terminal-panel")).toBeInTheDocument();
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
  });

  it("reports the open desk, and none once it is closed or the floor is left", async () => {
    const user = userEvent.setup();
    const api = new MockApi(makeSeedSnapshot());
    render(<App api={api} />);
    const { id, worker } = await seatHire(api);
    await user.click(worker());
    await waitFor(() => expect(api.openDesk).toBe(id));
    await user.click(await screen.findByTestId("terminal-close"));
    await waitFor(() => expect(api.openDesk).toBeNull());
    await user.click(worker());
    await waitFor(() => expect(api.openDesk).toBe(id));
    await user.click(screen.getByTestId("back-button"));
    await waitFor(() => expect(api.openDesk).toBeNull());
  });
});
