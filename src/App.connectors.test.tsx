import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";
import { makeDemoSnapshot } from "./api/mockData";

// jsdom has no canvas: the terminal is never opened here, so a bare stand-in is enough.
vi.mock("@xterm/xterm", () => ({ Terminal: class {} }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class {} }));

async function enterShop(api: MockApi): Promise<void> {
  render(<App api={api} />);
  const storeys = await screen.findAllByTestId("storey");
  const shop = storeys.find((s) => s.getAttribute("data-project") === "p-shop");
  if (shop === undefined) throw new Error("no shop storey");
  await userEvent.click(within(shop).getByTestId("floor-nameplate"));
  await screen.findByTestId("back-button");
}

const linearApi = (): MockApi => new MockApi(makeDemoSnapshot());

describe("connectors in the app", () => {
  it("shows Linear items after the store items on both boards, with badges", async () => {
    await enterShop(linearApi());
    await userEvent.click(screen.getByTestId("rail-todo"));
    const texts = screen.getAllByTestId("todo-check").map((c) => c.closest("li")?.textContent ?? "");
    expect(texts[0]).toContain("Fix the orders ingest dedupe");
    expect(texts[1]).toContain("Add a retry to the orders ingest");
    expect(screen.getByRole("button", { name: "Open ENG-102 in Linear" })).toBeInTheDocument();
    expect(screen.getByTestId("connector-line")).toHaveTextContent("Linear, checked");
    await userEvent.click(screen.getByTestId("rail-human-todo"));
    expect(screen.getByRole("button", { name: "Open ENG-101 in Linear" })).toBeInTheDocument();
  });

  it("ticks a Linear item only after the confirmation", async () => {
    await enterShop(linearApi());
    await userEvent.click(screen.getByTestId("rail-todo"));
    const box = screen.getAllByTestId("todo-check")[1];
    if (box === undefined) throw new Error("no Linear item");
    await userEvent.click(box);
    expect(box).not.toBeChecked();
    await userEvent.click(screen.getByTestId("todo-confirm"));
    expect(await screen.findByTestId("toggle-done")).toHaveTextContent("Show 2 done");
    expect(screen.getAllByTestId("todo-check")).toHaveLength(1);
  });

  it("leaves a refused tick open and says why", async () => {
    const api = linearApi();
    api.failNextLinearTick("Linear said no");
    await enterShop(api);
    await userEvent.click(screen.getByTestId("rail-todo"));
    const box = screen.getAllByTestId("todo-check")[1];
    if (box === undefined) throw new Error("no Linear item");
    await userEvent.click(box);
    await userEvent.click(screen.getByTestId("todo-confirm"));
    expect(await screen.findByText(/Could not tick that off: Linear said no/)).toBeInTheDocument();
    expect(screen.getAllByTestId("todo-check")[1]).not.toBeChecked();
  });

  it("records the opened link and refreshes the line on Re-check", async () => {
    const api = linearApi();
    await enterShop(api);
    await userEvent.click(screen.getByTestId("rail-todo"));
    await userEvent.click(screen.getByRole("button", { name: "Open ENG-102 in Linear" }));
    expect(api.openedLinks).toEqual(["p-shop|linear|issue-2"]);
    expect(screen.getByTestId("connector-line")).toHaveTextContent(/checked \d+ days ago/);
    await userEvent.click(screen.getByTestId("connector-recheck"));
    await waitFor(() => expect(screen.getByTestId("connector-line")).toHaveTextContent("checked under a minute ago"));
  });

  it("opens the Connectors group with Cmd+, and shows a saved key", async () => {
    render(<App api={new MockApi()} />);
    await screen.findAllByTestId("storey");
    fireEvent.keyDown(window, { key: ",", metaKey: true });
    const dialog = await screen.findByTestId("settings-dialog");
    expect(within(dialog).getByRole("heading", { name: "Connectors" })).toBeInTheDocument();
    expect(await within(dialog).findByTestId("linear-key-status")).toHaveTextContent("No Linear API key is saved.");
    await userEvent.type(within(dialog).getByTestId("linear-key"), "test-key");
    await userEvent.click(within(dialog).getByTestId("linear-key-save"));
    await waitFor(() => expect(within(dialog).getByTestId("linear-key-status")).toHaveTextContent("It belongs to Demo User."));
  });
  it("connects a floor from the Info panel, changes the link and disconnects", async () => {
    const api = new MockApi();
    await api.setLinearKey("test-key");
    render(<App api={api} />);
    const storeys = await screen.findAllByTestId("storey");
    const notes = storeys.find((s) => s.getAttribute("data-project") === "p-notes");
    if (notes === undefined) throw new Error("no notes storey");
    await userEvent.click(within(notes).getByTestId("floor-nameplate"));
    await screen.findByTestId("back-button");
    await userEvent.click(screen.getByTestId("rail-info"));

    await userEvent.click(await screen.findByTestId("linear-connect"));
    await userEvent.selectOptions(await screen.findByTestId("linear-team"), "team-eng");
    await userEvent.click(screen.getByTestId("linear-submit"));
    expect(await screen.findByTestId("linear-summary")).toHaveTextContent("ENG · Engineering");
    expect(await screen.findByText("Connected to Linear team ENG.")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("rail-todo"));
    expect(screen.getByRole("button", { name: "Open ENG-102 in Linear" })).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("rail-human-todo"));
    expect(screen.getByRole("button", { name: "Open ENG-101 in Linear" })).toBeInTheDocument();

    await userEvent.click(screen.getByTestId("rail-info"));
    await userEvent.click(screen.getByTestId("linear-change"));
    await userEvent.selectOptions(await screen.findByTestId("linear-team"), "team-ops");
    await userEvent.click(screen.getByTestId("linear-submit"));
    expect(await screen.findByTestId("linear-summary")).toHaveTextContent("OPS · Operations");

    await userEvent.click(screen.getByTestId("linear-disconnect"));
    expect(await screen.findByTestId("linear-connect")).toBeInTheDocument();
    expect(screen.queryByTestId("linear-summary")).toBeNull();
    await userEvent.click(screen.getByTestId("rail-human-todo"));
    expect(screen.queryByRole("button", { name: /in Linear/ })).toBeNull();
  });

  it("asks for a key in Settings when none is saved", async () => {
    render(<App api={new MockApi()} />);
    const storeys = await screen.findAllByTestId("storey");
    const notes = storeys.find((s) => s.getAttribute("data-project") === "p-notes");
    if (notes === undefined) throw new Error("no notes storey");
    await userEvent.click(within(notes).getByTestId("floor-nameplate"));
    await userEvent.click(await screen.findByTestId("rail-info"));
    await userEvent.click(await screen.findByTestId("linear-connect"));
    await userEvent.click(await screen.findByTestId("linear-open-settings"));
    expect(await screen.findByTestId("settings-dialog")).toBeInTheDocument();
  });
});
