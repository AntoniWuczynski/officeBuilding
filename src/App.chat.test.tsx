import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";

// jsdom has no canvas: the terminal screen only needs to exist.
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options = {};
    loadAddon(): void {}
    open(): void {}
    write(_data: string, done?: () => void): void {
      done?.();
    }
    resize(): void {}
    onData(): { dispose(): void } {
      return { dispose: () => {} };
    }
    onResize(): { dispose(): void } {
      return { dispose: () => {} };
    }
    focus(): void {}
    dispose(): void {}
  },
}));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit(): void {}
  },
}));

/** Open the arcade floor and hire Claude Code from the dialog for `task`, then sit the hire down. */
async function hireClaude(api: MockApi, task: string): Promise<void> {
  const user = userEvent.setup();
  render(<App api={api} />);
  await screen.findAllByTestId("storey");
  const storey = screen.getAllByTestId("storey").find((s) => s.getAttribute("data-project") === "p-arcade");
  if (storey === undefined) throw new Error("no arcade storey");
  await user.click(within(storey).getByTestId("floor-nameplate"));
  await user.click(await screen.findByTestId("hire-desk"));
  const dialog = screen.getByTestId("hire-dialog");
  await within(dialog).findByTestId("hire-model");
  await user.type(within(dialog).getByRole("textbox"), task);
  await user.click(screen.getByTestId("hire-submit"));
  fireEvent.animationEnd(await screen.findByTestId("walk"));
}

async function openDesk(task: string): Promise<void> {
  await userEvent.setup().click(await screen.findByRole("button", { name: new RegExp(task) }));
}

describe("App chat view", () => {
  it("opens the conversation of a Claude Code hire when the setting is Chat", async () => {
    const api = new MockApi();
    await api.updateSettings({ agents: { view: "chat" } });
    await hireClaude(api, "tidy the notes");
    await openDesk("tidy the notes");
    expect(await screen.findByTestId("conversation")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Chat" })).toHaveAttribute("aria-pressed", "true");
  });

  it("opens the terminal when the setting is Terminal", async () => {
    await hireClaude(new MockApi(), "tidy the notes");
    await openDesk("tidy the notes");
    expect(await screen.findByTestId("terminal-screen")).toBeInTheDocument();
    expect(screen.queryByTestId("conversation")).toBeNull();
  });

  it("shows a message sent while the agent waits on its terminal card as waiting in the open panel", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    await api.updateSettings({ agents: { view: "chat" } });
    await hireClaude(api, "run the tests");
    await openDesk("run the tests");
    expect(await screen.findByText("Claude Code is waiting for your answer")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Message to the agent"), "and then lint{Enter}");
    const queue = await screen.findByTestId("chat-queue");
    expect(queue).toHaveTextContent("and then lint");
    expect(queue).toHaveTextContent("Waiting: it is waiting on you.");
  });
});
