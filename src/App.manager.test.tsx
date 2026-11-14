import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";

async function enter(projectId: string): Promise<void> {
  const storeys = await screen.findAllByTestId("storey");
  const storey = storeys.find((s) => s.getAttribute("data-project") === projectId);
  if (storey === undefined) throw new Error(`no storey ${projectId}`);
  await userEvent.click(within(storey).getByTestId("floor-nameplate"));
  await screen.findByTestId("back-button");
}

describe("the manager from the floor", () => {
  it("opens the Manager section from the sign and hires, with a toast", async () => {
    const api = new MockApi();
    const spy = vi.spyOn(api, "hireManager");
    render(<App api={api} />);
    await enter("p-notes");
    await userEvent.click(screen.getByTestId("object-manager"));
    expect(await screen.findByTestId("sidebar")).toHaveAccessibleName("Manager");
    await userEvent.click(screen.getByTestId("manager-hire"));
    expect(spy).toHaveBeenCalledWith("p-notes");
    expect(await screen.findByText("Hired a manager for this floor. The floor is pinned.")).toBeInTheDocument();
    expect(await screen.findByTestId("manager-paused")).toBeInTheDocument();
    expect(screen.queryByTestId("object-manager")).not.toBeInTheDocument();
  });

  it("toasts and keeps the error when hiring fails", async () => {
    const api = new MockApi();
    vi.spyOn(api, "hireManager").mockRejectedValue(new Error("claude is not installed"));
    render(<App api={api} />);
    await enter("p-notes");
    await userEvent.click(screen.getByTestId("rail-manager"));
    await userEvent.click(screen.getByTestId("manager-hire"));
    expect(await screen.findByText("Could not hire a manager: claude is not installed.")).toBeInTheDocument();
    expect(screen.getByTestId("manager-hire-error")).toHaveTextContent("claude is not installed");
  });

  it("pauses and resumes the manager, and lets it hire, each with a toast", async () => {
    const api = new MockApi();
    render(<App api={api} />);
    await enter("p-shop");
    await userEvent.click(screen.getByTestId("rail-manager"));
    await userEvent.click(screen.getByRole("switch", { name: "Paused" }));
    expect(await screen.findByText("The manager is paused. Nothing is sent or hired until you resume it.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: "Paused" }));
    expect(await screen.findByText("The manager is back at work.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: "May hire desks" }));
    expect(await screen.findByText("The manager may no longer hire desks on this floor.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: "May hire desks" }));
    expect(await screen.findByText("The manager may hire desks on this floor.")).toBeInTheDocument();
  });

  it("toasts a switch that fails", async () => {
    const api = new MockApi();
    vi.spyOn(api, "setManagerPaused").mockRejectedValue(new Error("no such floor"));
    render(<App api={api} />);
    await enter("p-shop");
    await userEvent.click(screen.getByTestId("rail-manager"));
    await userEvent.click(screen.getByRole("switch", { name: "Paused" }));
    expect(await screen.findByText("Could not change the manager: no such floor.")).toBeInTheDocument();
  });

  it("withdraws a waiting note, and toasts when that fails", async () => {
    const api = new MockApi();
    render(<App api={api} />);
    await enter("p-shop");
    await userEvent.click(screen.getByTestId("rail-manager"));
    expect(screen.getByTestId("rail-manager")).toHaveTextContent("1");
    vi.spyOn(api, "withdrawManagerNote").mockRejectedValueOnce(new Error("that note is no longer waiting"));
    await userEvent.click(screen.getByTestId("manager-withdraw"));
    expect(await screen.findByText("Could not withdraw that note: that note is no longer waiting.")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("manager-withdraw"));
    expect(await screen.findByText("No notes are waiting.")).toBeInTheDocument();
  });

  it("opens the TODO section from the plan", async () => {
    render(<App api={new MockApi()} />);
    await enter("p-shop");
    await userEvent.click(screen.getByTestId("rail-manager"));
    await userEvent.click(screen.getByTestId("manager-open-todo"));
    expect(await screen.findByTestId("sidebar")).toHaveAccessibleName("TODO");
  });

  it("opens Settings from the panel, which closes the section", async () => {
    render(<App api={new MockApi()} />);
    await enter("p-notes");
    await userEvent.click(screen.getByTestId("rail-manager"));
    await userEvent.click(screen.getByTestId("manager-settings"));
    expect(await screen.findByRole("dialog", { name: /settings/i })).toBeInTheDocument();
    // The sidebar slides out first.
    await waitFor(() => expect(screen.queryByTestId("sidebar")).not.toBeInTheDocument());
  });
});
