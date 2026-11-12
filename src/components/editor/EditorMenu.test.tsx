import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { EditorMenu } from "./EditorMenu";

const items = [
  { editor: "cursor", label: "Open in Cursor", name: "Cursor" },
  { editor: "zed", label: "Open in Zed", name: "Zed" },
  { editor: "vs-code", label: "Open in VS Code", name: "VS Code" },
] as const;

function show(over: { caption?: string | null; items?: typeof items | readonly [] } = {}): { onPick: ReturnType<typeof vi.fn>; onClose: ReturnType<typeof vi.fn> } {
  const onPick = vi.fn();
  const onClose = vi.fn();
  render(<EditorMenu at={{ x: 10, y: 10 }} caption={over.caption ?? null} items={over.items ?? items} onPick={onPick} onClose={onClose} />);
  return { onPick, onClose };
}

describe("EditorMenu", () => {
  it("focuses the first line and picks on click", () => {
    const { onPick } = show();
    const lines = screen.getAllByTestId("editor-menu-item");
    expect(lines[0]).toHaveFocus();
    fireEvent.click(lines[1] ?? document.body);
    expect(onPick).toHaveBeenCalledWith("zed");
  });

  it("moves focus with the arrow keys and wraps", () => {
    show();
    const lines = screen.getAllByTestId("editor-menu-item");
    fireEvent.keyDown(lines[0] ?? document.body, { key: "ArrowUp" });
    expect(lines[2]).toHaveFocus();
    fireEvent.keyDown(lines[2] ?? document.body, { key: "ArrowDown" });
    expect(lines[0]).toHaveFocus();
    fireEvent.keyDown(lines[0] ?? document.body, { key: "ArrowDown" });
    expect(lines[1]).toHaveFocus();
  });

  it("shows the caption and the empty text", () => {
    show({ caption: "Opens its worktree on branch x", items: [] });
    expect(screen.getByText("Opens its worktree on branch x")).toBeInTheDocument();
    expect(screen.getByText("No editor was found on this Mac.")).toBeInTheDocument();
    expect(screen.queryByTestId("editor-menu-item")).not.toBeInTheDocument();
  });

  it("closes on Escape", () => {
    const { onClose } = show();
    fireEvent.keyDown(screen.getAllByTestId("editor-menu-item")[0] ?? document.body, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes on a press outside, not inside", () => {
    const { onClose } = show();
    fireEvent.pointerDown(screen.getByTestId("editor-menu"));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.pointerDown(document.body);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it.each([
    ["blur", () => fireEvent.blur(window)],
    ["resize", () => fireEvent(window, new Event("resize"))],
    ["wheel", () => fireEvent.wheel(window)],
  ])("closes on window %s and stops listening once gone", (_name, trigger) => {
    const { onClose } = show();
    trigger();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("removes its listeners when unmounted", () => {
    const onClose = vi.fn();
    const { unmount } = render(<EditorMenu at={{ x: 0, y: 0 }} caption={null} items={items} onPick={vi.fn()} onClose={onClose} />);
    unmount();
    fireEvent.pointerDown(document.body);
    fireEvent.wheel(window);
    expect(onClose).not.toHaveBeenCalled();
  });
});
