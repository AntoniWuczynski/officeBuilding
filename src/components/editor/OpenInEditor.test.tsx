import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { EditorList, Place } from "../../types";
import { EditorContext } from "./EditorContext";
import { OpenInEditor } from "./OpenInEditor";

const place: Place = { kind: "session", sessionId: "s-1" };

function list(installed: readonly string[], defaultEditor: EditorList["defaultEditor"]): EditorList {
  return {
    editors: [
      { editor: "cursor", label: "Cursor", installed: installed.includes("cursor") },
      { editor: "vs-code", label: "VS Code", installed: installed.includes("vs-code") },
      { editor: "zed", label: "Zed", installed: installed.includes("zed") },
      { editor: "devin-desktop", label: "Devin Desktop", installed: installed.includes("devin-desktop") },
    ],
    defaultEditor,
  };
}

function show(editors: EditorList | null, props: { shortcut?: boolean } = {}): ReturnType<typeof vi.fn> {
  const open = vi.fn();
  render(
    <EditorContext value={{ list: editors, open }}>
      <OpenInEditor place={place} what="“audit”" opens="its worktree on branch x" {...props} />
    </EditorContext>,
  );
  return open;
}

describe("OpenInEditor", () => {
  it("is labelled with the default editor and opens it", async () => {
    const open = show(list(["cursor"], "cursor"), { shortcut: true });
    const main = screen.getByTestId("open-in-main");
    expect(main).toHaveTextContent("Open in Cursor");
    expect(main).toHaveAttribute("title", "Open its worktree on branch x in Cursor (Cmd+Shift+O)");
    await userEvent.click(main);
    expect(open).toHaveBeenCalledWith(place, "cursor", "“audit”");
  });

  it("says Open in editor before the list loads and lets the backend choose", async () => {
    const open = show(null);
    expect(screen.getByTestId("open-in-main")).toHaveTextContent("Open in editor");
    expect(screen.getByTestId("open-in-main")).toHaveAttribute("title", "Open its worktree on branch x in your editor");
    await userEvent.click(screen.getByTestId("open-in-main"));
    expect(open).toHaveBeenCalledWith(place, null, "“audit”");
    expect(screen.queryByTestId("open-in-more")).not.toBeInTheDocument();
  });

  it("is disabled with nothing installed", () => {
    show(list([], null));
    const main = screen.getByTestId("open-in-main");
    expect(main).toBeDisabled();
    expect(main).toHaveTextContent("Open in editor");
    expect(main).toHaveAttribute("title", "No editor was found on this Mac. The building looks for Cursor, VS Code, Zed and Devin Desktop.");
  });

  it("shows the chevron only with a second editor", () => {
    show(list(["cursor"], "cursor"));
    expect(screen.queryByTestId("open-in-more")).not.toBeInTheDocument();
  });

  it("lists the other editors under the chevron and opens the one picked", async () => {
    const user = userEvent.setup();
    const open = show(list(["cursor", "zed"], "cursor"));
    const more = screen.getByTestId("open-in-more");
    expect(more).toHaveAttribute("aria-expanded", "false");
    await user.click(more);
    expect(more).toHaveAttribute("aria-expanded", "true");
    const lines = screen.getAllByTestId("editor-menu-item");
    expect(lines.map((l) => l.textContent)).toEqual(["Open in Zed"]);
    await user.click(lines[0] ?? document.body);
    expect(open).toHaveBeenCalledWith(place, "zed", "“audit”");
    expect(screen.queryByTestId("editor-menu")).not.toBeInTheDocument();
  });

  it("closes the menu on Escape, an outside press and a second chevron click", async () => {
    const user = userEvent.setup();
    show(list(["cursor", "zed"], "cursor"));
    const more = screen.getByTestId("open-in-more");
    await user.click(more);
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("editor-menu")).not.toBeInTheDocument();
    await user.click(more);
    fireEvent.pointerDown(document.body);
    expect(screen.queryByTestId("editor-menu")).not.toBeInTheDocument();
    await user.click(more);
    await user.click(more);
    expect(screen.queryByTestId("editor-menu")).not.toBeInTheDocument();
  });

  it("renders nothing without a provider", () => {
    const { container } = render(<OpenInEditor place={place} what="x" opens="y" />);
    expect(container).toBeEmptyDOMElement();
  });
});
