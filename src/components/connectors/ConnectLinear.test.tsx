import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { LinearKeyStatus, LinearLink, LinearTeam } from "../../types";
import { MOCK_TEAMS } from "../../api/mockConnectors";
import { ConnectorsContext } from "./ConnectorsContext";
import type { ConnectorActions } from "./ConnectorsContext";
import { ConnectLinear } from "./ConnectLinear";

const SAVED: LinearKeyStatus = { kind: "saved", account: null };

function actionsFor(over: Partial<ConnectorActions> = {}): ConnectorActions {
  return {
    keyStatus: () => Promise.resolve(SAVED),
    setKey: vi.fn(),
    teams: () => Promise.resolve<readonly LinearTeam[]>(MOCK_TEAMS),
    connect: vi.fn(() => Promise.resolve()),
    disconnect: vi.fn(),
    recheck: vi.fn(),
    openLink: vi.fn(),
    openSettings: vi.fn(),
    ...over,
  };
}

function show(actions: ConnectorActions, current: LinearLink | null = null, onDone = vi.fn()): typeof onDone {
  render(
    <ConnectorsContext value={actions}>
      <ConnectLinear projectId="p-1" current={current} onDone={onDone} />
    </ConnectorsContext>,
  );
  return onDone;
}

describe("ConnectLinear", () => {
  it("sends the user to Settings when no key is saved", async () => {
    const actions = actionsFor({ keyStatus: () => Promise.resolve({ kind: "missing" }) });
    show(actions);
    expect(await screen.findByText("Add a Linear API key in Settings first.")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("linear-open-settings"));
    expect(actions.openSettings).toHaveBeenCalledOnce();
  });

  it.each<[LinearKeyStatus, string]>([
    [{ kind: "rejected" }, "Linear did not accept the saved key. Replace it in Settings."],
    [{ kind: "unavailable", reason: "locked" }, "The Keychain did not hand over the key: locked."],
  ])("says %j", async (status, sentence) => {
    show(actionsFor({ keyStatus: () => Promise.resolve(status) }));
    expect(await screen.findByText(sentence)).toBeInTheDocument();
  });

  it("lists a team's projects and resets the project when the team changes", async () => {
    show(actionsFor());
    const team = await screen.findByTestId("linear-team");
    expect(screen.getByTestId("linear-submit")).toBeDisabled();
    await userEvent.selectOptions(team, "team-eng");
    expect(screen.getByRole("option", { name: "Checkout" })).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByTestId("linear-project"), "proj-checkout");
    expect(screen.getByTestId("linear-project")).toHaveValue("proj-checkout");
    await userEvent.selectOptions(team, "team-ops");
    expect(screen.getByTestId("linear-project")).toHaveValue("");
    expect(screen.queryByRole("option", { name: "Checkout" })).toBeNull();
  });

  it("connects with the label trimmed, and null for a blank one", async () => {
    const actions = actionsFor();
    const onDone = show(actions);
    await userEvent.selectOptions(await screen.findByTestId("linear-team"), "team-eng");
    await userEvent.selectOptions(screen.getByTestId("linear-project"), "proj-tiles");
    await userEvent.type(screen.getByTestId("linear-label"), "  bug  ");
    await userEvent.click(screen.getByTestId("linear-submit"));
    expect(actions.connect).toHaveBeenCalledWith("p-1", {
      kind: "linear",
      teamId: "team-eng",
      teamKey: "ENG",
      teamName: "Engineering",
      project: { id: "proj-tiles", name: "Map tiles" },
      label: "bug",
    });
    await vi.waitFor(() => expect(onDone).toHaveBeenCalledOnce());
  });

  it("sends null for a blank label and no project", async () => {
    const actions = actionsFor();
    show(actions);
    await userEvent.selectOptions(await screen.findByTestId("linear-team"), "team-ops");
    await userEvent.click(screen.getByTestId("linear-submit"));
    expect(actions.connect).toHaveBeenCalledWith("p-1", expect.objectContaining({ teamKey: "OPS", project: null, label: null }));
  });

  it("stays in the form when connecting fails", async () => {
    const onDone = show(actionsFor({ connect: () => Promise.reject(new Error("floor is gone")) }));
    await userEvent.selectOptions(await screen.findByTestId("linear-team"), "team-eng");
    await userEvent.click(screen.getByTestId("linear-submit"));
    expect(await screen.findByText("Could not connect: floor is gone.")).toBeInTheDocument();
    expect(onDone).not.toHaveBeenCalled();
    expect(screen.getByTestId("linear-submit")).toBeEnabled();
  });

  it("offers Try again when the team list fails", async () => {
    const teams = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(MOCK_TEAMS);
    show(actionsFor({ teams }));
    expect(await screen.findByText("Could not read your Linear teams: offline.")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("linear-retry"));
    expect(await screen.findByTestId("linear-team")).toBeInTheDocument();
  });

  it("says so when the key sees no team", async () => {
    show(actionsFor({ teams: () => Promise.resolve([]) }));
    expect(await screen.findByText("This key cannot see any Linear team.")).toBeInTheDocument();
  });

  it("preselects the current link and cancels", async () => {
    const current: LinearLink = { kind: "linear", teamId: "team-eng", teamKey: "ENG", teamName: "Engineering", project: { id: "proj-tiles", name: "Map tiles" }, label: "agent" };
    const onDone = show(actionsFor(), current);
    expect(await screen.findByTestId("linear-team")).toHaveValue("team-eng");
    expect(screen.getByTestId("linear-project")).toHaveValue("proj-tiles");
    expect(screen.getByTestId("linear-label")).toHaveValue("agent");
    await userEvent.click(screen.getByTestId("linear-cancel"));
    expect(onDone).toHaveBeenCalledOnce();
  });

  it("renders nothing without a provider", () => {
    const { container } = render(<ConnectLinear projectId="p-1" current={null} onDone={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });
});
