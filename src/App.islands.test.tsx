import { describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";

function renderApp(): void {
  render(<App api={new MockApi()} />);
}

describe("island view", () => {
  // The island map replaces only the building drawing: every building-view ability must still be there.
  async function showIslands(): Promise<ReturnType<typeof userEvent.setup>> {
    localStorage.clear();
    const user = userEvent.setup();
    renderApp();
    await screen.findAllByTestId("storey");
    await user.click(screen.getByTestId("view-islands"));
    await screen.findByTestId("archipelago");
    return user;
  }

  it("keeps the top bar, the usage signs and one island per floor", async () => {
    await showIslands();
    expect(screen.queryAllByTestId("storey")).toHaveLength(0);
    expect(screen.getAllByTestId("island-nameplate")).toHaveLength(4);
    expect(screen.getByTestId("tally")).toHaveTextContent("2/6");
    expect(screen.getByRole("complementary", { name: "Usage limits" })).toBeInTheDocument();
    expect(screen.getByTestId("view-islands")).toHaveAttribute("aria-pressed", "true");
    // jsdom has no WebGL: the map says so instead of failing, and the list still works.
    expect(await screen.findByText(/needs WebGL/)).toBeInTheDocument();
    localStorage.clear();
  });

  it("dims islands that do not match the search", async () => {
    const user = await showIslands();
    await user.type(screen.getByPlaceholderText("Search islands"), "arcade");
    const dim = screen.getAllByTestId("island-nameplate").filter((b) => b.closest(".arch-list-row")?.classList.contains("is-dim"));
    expect(dim).toHaveLength(3);
    localStorage.clear();
  });

  it("goes straight into an island that needs you and comes back to the islands", async () => {
    const user = await showIslands();
    await user.click(screen.getByRole("button", { name: "corner_shop" }));
    expect(await screen.findByRole("heading", { name: "corner_shop" })).toBeInTheDocument();
    expect(screen.getByTestId("back-button")).toHaveTextContent("Islands");
    await user.click(screen.getByTestId("back-button"));
    expect(await screen.findByTestId("archipelago")).toBeInTheDocument();
    localStorage.clear();
  });

  it("adds and demolishes islands with the building's own dialogs, and renames from the sign", async () => {
    const user = await showIslands();
    await user.click(screen.getByTestId("add-island"));
    expect(screen.getByRole("heading", { name: "Add an island" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Sink arcade" }));
    expect(screen.getByRole("heading", { name: "Sink arcade?" })).toBeInTheDocument();
    expect(screen.getByTestId("demolish-submit")).toHaveTextContent("Sink");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByTestId("roof-sign")).toHaveTextContent("The Islands");
    await user.click(screen.getByTestId("roof-sign"));
    await user.keyboard("{Control>}a{/Control}Acme HQ{Enter}");
    expect(document.querySelector(".topbar-sign")).toHaveTextContent("Acme HQ");
    localStorage.clear();
  });

  it("picks an island from the list, lists its agents, and opens it", async () => {
    const user = await showIslands();
    const plate = screen.getByRole("button", { name: /^corner_shop: 1 of 3 active, .+\. Show island\.$/ });
    expect(plate).toHaveAttribute("aria-expanded", "false");
    await user.click(plate);
    const panel = screen.getByRole("complementary", { name: "corner_shop island" });
    expect(plate).toHaveAttribute("aria-expanded", "true");
    expect(plate).toHaveAttribute("aria-controls", panel.id);
    // Focus moves into the panel, so its agents and Open island come next in Tab order.
    expect(within(panel).getByRole("heading", { name: "corner_shop" })).toHaveFocus();
    expect(within(panel).getByText("audit checkout")).toBeInTheDocument();
    expect(within(panel).getByText("map-tiles mirror")).toBeInTheDocument();
    await user.click(within(panel).getByRole("button", { name: "Open island" }));
    expect(await screen.findByRole("heading", { name: "corner_shop" })).toBeInTheDocument();
    localStorage.clear();
  });

  it("shows a floor's QA stamp on its island's nameplate and reads it out", async () => {
    await showIslands();
    const plate = screen.getAllByTestId("island-nameplate").find((b) => b.getAttribute("data-project") === "p-shop");
    if (plate === undefined) throw new Error("no island p-shop");
    expect(within(plate).getByTestId("qa-stamp")).toHaveTextContent("QA notes");
    expect(plate).toHaveAttribute("aria-label", expect.stringContaining(", QA found problems. Show island."));
    localStorage.clear();
  });

  it("closes Settings when the Add an island or Sink plate opens", async () => {
    const user = await showIslands();
    const cmdComma = (): boolean => fireEvent.keyDown(window, { key: ",", metaKey: true });
    cmdComma();
    await screen.findByTestId("settings-dialog");
    await user.click(screen.getByTestId("add-island"));
    expect(screen.getByTestId("add-floor-dialog")).toBeInTheDocument();
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    cmdComma();
    await screen.findByTestId("settings-dialog");
    await user.click(screen.getByRole("button", { name: "Sink arcade" }));
    expect(screen.getByTestId("demolish-dialog")).toBeInTheDocument();
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
    localStorage.clear();
  });

  it("remembers the last view picked", async () => {
    await showIslands();
    cleanup();
    renderApp();
    expect(await screen.findByTestId("archipelago")).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(screen.getByTestId("view-office"));
    expect(await screen.findAllByTestId("storey")).toHaveLength(4);
    localStorage.clear();
  });
});
