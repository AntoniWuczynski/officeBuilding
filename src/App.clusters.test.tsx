import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";
import { makeSeedSnapshot } from "./api/mockData";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const plates = (): string[] => screen.getAllByTestId("floor-nameplate").map((p) => p.getAttribute("data-project") ?? "");

describe("project clusters in the app", () => {
  it("remember a guessed project across a visit to a floor, though one of its two sessions ended there", async () => {
    const seed = makeSeedSnapshot();
    const shop = seed.projects.find((p) => p.id === "p-shop");
    const office = seed.projects.find((p) => p.id === "p-office");
    if (shop === undefined || office === undefined) throw new Error("the seed has both floors");
    const named = (id: string, name: string) => {
      const s = seed.sessions.find((x) => x.id === id);
      if (s === undefined) throw new Error(`the seed has ${id}`);
      return { ...s, sessionName: name, sessionNameSource: "user" as const };
    };
    // The beta repo stands second in the building, so grouping it first is the only thing that reorders the floors.
    const snap = {
      ...seed,
      projects: [{ ...office, sessionIds: [] }, { ...shop, sessionIds: ["s-1", "s-2"] }],
      sessions: [named("s-1", "beta-web"), named("s-2", "beta-api")],
      questions: [],
    };
    const api = new MockApi(snap);
    const user = userEvent.setup();
    render(<App api={api} />);
    await screen.findAllByTestId("floor-nameplate");
    expect(plates()).toEqual(["p-shop", "p-office"]);

    await user.click(screen.getAllByTestId("storey").find((s) => s.getAttribute("data-project") === "p-shop") ?? document.body);
    await screen.findByTestId("back-button");
    await act(() => api.exileSession("s-2"));
    await user.click(screen.getByTestId("back-button"));

    await waitFor(() => expect(plates()).toEqual(["p-shop", "p-office"]));
  });
});
