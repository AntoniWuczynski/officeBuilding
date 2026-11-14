import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeRoutingSettings, MockRouting } from "../../api/mockRouting";
import { makeSettings } from "../../api/mockSettings";
import type { Project, RoutingSettings } from "../../types";
import { FloorContextRow } from "./FloorContextRow";
import { RoutingContext } from "./RoutingContext";
import type { RoutingApi } from "../../api/RoutingApi";

const PROJECT: Project = { id: "floor-1", name: "Demo", path: "/demo", sessionIds: [] };
const WITH_CONTEXTS: RoutingSettings = {
  ...makeRoutingSettings(),
  enabled: true,
  contexts: [
    { id: "ctx-a", name: "Work", chain: [] },
    { id: "ctx-b", name: "Home", chain: [] },
  ],
};

function show(routing: RoutingSettings, api?: RoutingApi): RoutingApi {
  const provided = api ?? new MockRouting(() => Promise.resolve({ ...makeSettings(), routing }));
  render(
    <RoutingContext value={provided}>
      <dl>
        <FloorContextRow project={PROJECT} />
      </dl>
    </RoutingContext>,
  );
  return provided;
}

describe("FloorContextRow", () => {
  it("renders nothing without a provider", () => {
    render(
      <dl>
        <FloorContextRow project={PROJECT} />
      </dl>,
    );
    expect(screen.queryByText("Account context")).not.toBeInTheDocument();
  });

  it("says there are no contexts yet", async () => {
    show(makeRoutingSettings());
    expect(await screen.findByText("No account contexts yet. Add one in Settings, under Routing.")).toBeInTheDocument();
  });

  it("chooses a context and goes back to Not set", async () => {
    const api = show(WITH_CONTEXTS);
    const select = await screen.findByTestId("floor-context");
    expect(screen.getByText("Desks on this floor start as they do today.")).toBeInTheDocument();
    await userEvent.selectOptions(select, "Work");
    expect(select).toHaveValue("ctx-a");
    expect((await api.floorRouting(PROJECT.id)).context).toBe("ctx-a");
    expect(screen.queryByText("Desks on this floor start as they do today.")).not.toBeInTheDocument();
    await userEvent.selectOptions(select, "Not set");
    expect(select).toHaveValue("");
    expect((await api.floorRouting(PROJECT.id)).context).toBeNull();
  });

  it("says when routing is off", async () => {
    show({ ...WITH_CONTEXTS, enabled: false });
    expect(await screen.findByText("Routing is switched off in Settings, so this has no effect yet.")).toBeInTheDocument();
  });

  it("shows a failed read", async () => {
    const api = new MockRouting(() => Promise.reject(new Error("boom")));
    show(WITH_CONTEXTS, api);
    expect(await screen.findByText("Could not read this floor’s account context: boom.")).toBeInTheDocument();
  });

  it("shows a failed change and keeps what is saved", async () => {
    const mock = new MockRouting(() => Promise.resolve({ ...makeSettings(), routing: WITH_CONTEXTS }));
    const api: RoutingApi = {
      keyStatus: (id) => mock.keyStatus(id),
      setKey: (id, key) => mock.setKey(id, key),
      floorRouting: (id) => mock.floorRouting(id),
      deskRoute: (id) => mock.deskRoute(id),
      retryRoute: (id, c) => mock.retryRoute(id, c),
      switchRoute: (id, c) => mock.switchRoute(id, c),
      setFloorContext: () => Promise.reject(new Error("that account context is no longer in Settings")),
    };
    show(WITH_CONTEXTS, api);
    const select = await screen.findByTestId("floor-context");
    await userEvent.selectOptions(select, "Work");
    expect(await screen.findByText("Could not change it: that account context is no longer in Settings.")).toBeInTheDocument();
    expect(select).toHaveValue("");
  });
});
