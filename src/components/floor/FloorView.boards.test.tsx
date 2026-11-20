import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { makeSeedSnapshot } from "../../api/mockData";
import { openCount, todosForProject } from "../../state/selectors";
import { floor } from "./floorHarness";

// The seed tags one board item "corner_shop" for the p-shop floor: it is one of that floor's Human TODOs.
describe("a floor's Human TODO counts the board lines tagged for it", () => {
  const seed = makeSeedSnapshot();
  const own = openCount(todosForProject(seed, "p-shop", "human"));

  it("agrees across the rail badge, the room's board and the panel when the floor has its own items too", () => {
    render(floor(seed, "p-shop", { section: "human-todo" }));
    const total = own + 1;
    expect(within(screen.getByTestId("rail-human-todo")).getByText(String(total))).toBeInTheDocument();
    expect(screen.getByTestId("object-human-todo")).toHaveAccessibleName(`Human TODO board: ${total} open ${total === 1 ? "item" : "items"}. Open Human TODO.`);
    expect(within(screen.getByTestId("sidebar")).getAllByRole("listitem")).toHaveLength(total);
  });

  it("shows a badge and a filled board when the tagged line is the floor's only open item", () => {
    render(floor({ ...seed, todos: seed.todos.filter((t) => !(t.projectId === "p-shop" && t.assignee === "human")) }, "p-shop"));
    expect(within(screen.getByTestId("rail-human-todo")).getByText("1")).toBeInTheDocument();
    expect(screen.getByTestId("object-human-todo")).toHaveAccessibleName("Human TODO board: 1 open item. Open Human TODO.");
    expect(screen.getByTestId("object-human-todo")).toHaveTextContent("Post the release note");
  });
});
