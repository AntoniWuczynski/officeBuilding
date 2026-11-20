import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { QueuedNote } from "../../types";
import { ManagerContext } from "./ManagerContext";
import type { ManagerActions } from "./ManagerContext";
import { ManagerNoteStrip } from "./ManagerNoteStrip";

const note = (id: string, text: string, over: Partial<QueuedNote> = {}): QueuedNote => ({ id, toSessionId: "s-1", text, queuedAt: "2026-09-28T15:36:00Z", hold: "busy", ...over });

function actionsFor(): ManagerActions {
  return { hire: vi.fn(), setPaused: vi.fn(), setHiring: vi.fn(), withdraw: vi.fn(), openSettings: vi.fn() };
}

function show(notes: readonly QueuedNote[], actions: ManagerActions | null = actionsFor(), tool: "codex" | "claude-code" = "claude-code"): void {
  render(
    <ManagerContext value={actions}>
      <ManagerNoteStrip projectId="p-shop" tool={tool} notes={notes} />
    </ManagerContext>,
  );
}

describe("ManagerNoteStrip", () => {
  it("renders nothing with no notes", () => {
    show([]);
    expect(screen.queryByTestId("manager-note-strip")).not.toBeInTheDocument();
  });

  it("shows the oldest note cut at 140 characters, the rest, the reason, and withdraws the oldest", async () => {
    const actions = actionsFor();
    const long = "x".repeat(150);
    show([note("n-1", long), note("n-2", "second", { hold: "paused" })], actions);
    const strip = screen.getByTestId("manager-note-strip");
    expect(strip).toHaveTextContent(`“${"x".repeat(140)}\u2026”.`);
    expect(strip).not.toHaveTextContent("x".repeat(141));
    expect(strip).toHaveTextContent("1 more behind it.");
    expect(strip).toHaveTextContent("Waiting: it is busy.");
    await userEvent.click(screen.getByTestId("manager-note-withdraw"));
    expect(actions.withdraw).toHaveBeenCalledWith("p-shop", "n-1");
  });

  it("says nothing of others for a single note, and uses the tool's words for a missing idle signal", () => {
    show([note("n-1", "short", { hold: "no-idle-signal" })], actionsFor(), "codex");
    const strip = screen.getByTestId("manager-note-strip");
    expect(strip).not.toHaveTextContent("more behind it");
    expect(strip).toHaveTextContent("Type /hooks in its terminal");
  });

  it("has no button without a provider", () => {
    show([note("n-1", "hello")], null);
    expect(screen.getByTestId("manager-note-strip")).toBeInTheDocument();
    expect(screen.queryByTestId("manager-note-withdraw")).not.toBeInTheDocument();
  });

  it("draws the text as text", () => {
    show([note("n-1", "<b>bold</b> [link](https://example.com)")]);
    const strip = screen.getByTestId("manager-note-strip");
    expect(strip).toHaveTextContent("<b>bold</b> [link](https://example.com)");
    expect(strip.querySelector("b")).toBeNull();
    expect(strip.querySelector("a")).toBeNull();
  });
});
