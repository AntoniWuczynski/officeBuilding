import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BoardsGroup } from "./BoardsGroup";

function show(sharedFiles: readonly string[] = []): ReturnType<typeof vi.fn> {
  const onChange = vi.fn();
  render(<BoardsGroup value={{ sharedFiles }} onChange={onChange} />);
  return onChange;
}

describe("BoardsGroup", () => {
  it("adds a full path after the ones already listed, trimmed, on Enter", async () => {
    const onChange = show(["/a/ONE.md"]);
    await userEvent.setup().type(screen.getByTestId("settings-boards-add"), "  ~/Desktop/TWO.md {Enter}");
    expect(onChange).toHaveBeenCalledWith({ sharedFiles: ["/a/ONE.md", "~/Desktop/TWO.md"] });
  });

  it("refuses a relative path and a file already listed, and says why", async () => {
    const onChange = show(["/a/ONE.md"]);
    const user = userEvent.setup();
    await user.type(screen.getByTestId("settings-boards-add"), "notes/ONE.md{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Use the file's full path, starting with / or ~/.");
    await user.clear(screen.getByTestId("settings-boards-add"));
    await user.type(screen.getByTestId("settings-boards-add"), "/a/ONE.md{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("That file is already on the list.");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("removes one file and keeps the order of the rest", async () => {
    const onChange = show(["/a/ONE.md", "/b/TWO.md", "/c/THREE.md"]);
    await userEvent.setup().click(screen.getByRole("button", { name: "Remove /b/TWO.md" }));
    expect(onChange).toHaveBeenCalledWith({ sharedFiles: ["/a/ONE.md", "/c/THREE.md"] });
  });
});
