import { describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BuildingView } from "./BuildingView";
import { makeSeedSnapshot } from "../../api/mockData";
import type { OfficeSnapshot, Project } from "../../types";

type Demolish = (projectId: string) => Promise<void>;

function view(snapshot: OfficeSnapshot, onDemolishFloor: Demolish = () => Promise.resolve()): React.ReactElement {
  return (
    <BuildingView
      snapshot={snapshot}
      onEnter={() => {}}
      canPickFolder={false}
      onPickFolder={() => Promise.resolve(null)}
      onAddFloor={(path) => Promise.resolve({ id: "p-new", name: path, path, sessionIds: [] })}
      onDemolishFloor={onDemolishFloor}
    />
  );
}

function storey(projectId: string): HTMLElement | undefined {
  return screen.queryAllByTestId("storey").find((s) => s.getAttribute("data-project") === projectId);
}

/** jsdom has no AnimationEvent: an `animationend` carrying the keyframes' name. */
function animationEnd(el: Element, animationName: string): void {
  const event = Object.assign(new Event("animationend", { bubbles: true }), { animationName });
  act(() => {
    el.dispatchEvent(event);
  });
}

function without(snapshot: OfficeSnapshot, projectId: string): OfficeSnapshot {
  return { ...snapshot, projects: snapshot.projects.filter((p) => p.id !== projectId), sessions: snapshot.sessions.filter((s) => s.projectId !== projectId) };
}

describe("demolishing a floor", () => {
  it("asks first, says its agents will be stopped, and demolishes on confirm", async () => {
    const user = userEvent.setup();
    const onDemolish = vi.fn<Demolish>().mockResolvedValue(undefined);
    render(view(makeSeedSnapshot(), onDemolish));
    await user.click(within(storey("p-shop") ?? document.body).getByTestId("demolish-floor"));
    const dialog = screen.getByTestId("demolish-dialog");
    expect(dialog).toHaveTextContent("Demolish corner_shop?");
    expect(dialog).toHaveTextContent("All 3 agents on this floor will be stopped first");
    expect(dialog).toHaveTextContent("A rebuilt floor keeps its sign-out sheet");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onDemolish).not.toHaveBeenCalled();
    await user.click(within(storey("p-notes") ?? document.body).getByTestId("demolish-floor"));
    expect(screen.getByTestId("demolish-dialog")).toHaveTextContent("No agents are working here.");
    await user.click(screen.getByTestId("demolish-submit"));
    expect(onDemolish).toHaveBeenCalledWith("p-notes");
    await waitFor(() => expect(screen.queryByTestId("demolish-dialog")).not.toBeInTheDocument());
  });

  it("keeps the dialog open with the reason when an agent will not stop", async () => {
    const user = userEvent.setup();
    render(view(makeSeedSnapshot(), () => Promise.reject(new Error("audit checkout did not stop, so the floor is still standing"))));
    await user.click(within(storey("p-shop") ?? document.body).getByTestId("demolish-floor"));
    await user.click(screen.getByTestId("demolish-submit"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not demolish the floor: audit checkout did not stop, so the floor is still standing.");
  });
});

describe("floors coming and going", () => {
  it("collapses a floor that goes, then clears it once the rubble is gone", () => {
    const seed = makeSeedSnapshot();
    const { rerender } = render(view(seed));
    expect(storey("p-arcade")).toHaveAttribute("data-phase", "here");
    rerender(view(without(seed, "p-arcade")));
    const falling = storey("p-arcade");
    expect(falling).toHaveAttribute("data-phase", "leaving");
    expect(falling?.querySelector(".rubble")).not.toBeNull();
    // The floors above hold their level until it is gone.
    expect(screen.getAllByTestId("storey")).toHaveLength(4);
    animationEnd(falling?.querySelector(".rubble") ?? document.body, "rubble-clear");
    expect(storey("p-arcade")).toBeUndefined();
    expect(screen.getAllByTestId("storey")).toHaveLength(3);
  });

  it("builds a floor that appears after first paint, behind scaffolding", () => {
    const seed = makeSeedSnapshot();
    const { rerender } = render(view(seed));
    const added: Project = { id: "p-new", name: "newapp", path: "~/code/newapp", sessionIds: [] };
    rerender(view({ ...seed, projects: [...seed.projects, added] }));
    expect(storey("p-new")).toHaveAttribute("data-phase", "arriving");
    expect(storey("p-new")?.querySelector(".scaffold")).not.toBeNull();
    expect(storey("p-shop")).toHaveAttribute("data-phase", "here");
  });

  it("walks an agent whose session ended off its storey", () => {
    const seed = makeSeedSnapshot();
    const { rerender } = render(view(seed));
    rerender(view({ ...seed, sessions: seed.sessions.filter((s) => s.id !== "s-6") }));
    const leaver = within(storey("p-arcade") ?? document.body).getByTestId("storey-leaver");
    animationEnd(leaver, "storey-leave");
    expect(screen.queryByTestId("storey-leaver")).not.toBeInTheDocument();
    expect(storey("p-arcade")).toHaveAttribute("data-phase", "here");
  });
});
