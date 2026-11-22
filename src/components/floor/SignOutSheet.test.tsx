import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { makeSeedSnapshot } from "../../api/mockData";
import type { EndedDesk } from "../../types";
import { SignOutSheet } from "./SignOutSheet";

const NOW = new Date("2026-09-28T15:45:00Z");
const seed = makeSeedSnapshot();
const ended = seed.ended.filter((e) => e.projectId === "p-shop");
const first = ended[0];
if (first === undefined) throw new Error("the seed has no ended session");

function show(record: EndedDesk | undefined): void {
  render(<SignOutSheet ended={ended} handovers={[]} now={NOW} endedBy={(id) => (id === first?.id ? record : undefined)} onResume={vi.fn(() => Promise.resolve())} />);
}

describe("the sign-out sheet's ended line", () => {
  it("says a manager ended a desk, and why", () => {
    show({ sessionId: first.id, at: "2026-09-28T15:10:00Z", reason: "It was done", how: "stopped" });
    const line = screen.getByTestId("signed-out-ended");
    expect(line).toHaveTextContent("Ended by the floor's manager 35 min ago: It was done.");
    expect(line).not.toHaveTextContent("forced");
    expect(screen.getAllByTestId("signed-out-row")).toHaveLength(ended.length);
    expect(screen.getAllByTestId("signed-out-ended")).toHaveLength(1);
  });

  it("adds the forced sentence only when the desk was forced", () => {
    show({ sessionId: first.id, at: "2026-09-28T15:10:00Z", reason: "It was looping", how: "forced" });
    expect(screen.getByTestId("signed-out-ended")).toHaveTextContent("It was looping. It did not stop when asked and was forced.");
  });

  it("shows no line for a session no manager ended", () => {
    show(undefined);
    expect(screen.queryByTestId("signed-out-ended")).not.toBeInTheDocument();
  });

  it("still shows the line when the end is stamped long after the session last did anything", () => {
    show({ sessionId: first.id, at: "2026-09-28T15:40:00Z", reason: "It sat idle", how: "stopped" });
    expect(screen.getByTestId("signed-out-ended")).toHaveTextContent("Ended by the floor's manager 5 min ago: It sat idle.");
  });

  it("shows the line when the end's whole-second stamp is cut below the last activity", () => {
    render(<SignOutSheet ended={[{ ...first, endedAt: "2026-09-28T15:10:00.400Z" }]} handovers={[]} now={NOW} endedBy={() => ({ sessionId: first.id, at: "2026-09-28T15:10:00Z", reason: "It was done", how: "stopped" })} onResume={vi.fn(() => Promise.resolve())} />);
    expect(screen.getByTestId("signed-out-ended")).toBeInTheDocument();
  });

  it("does not blame the manager for a later sign-out of the same session", () => {
    show({ sessionId: first.id, at: "2026-09-28T09:00:00Z", reason: "It was done", how: "stopped" });
    expect(screen.queryByTestId("signed-out-ended")).not.toBeInTheDocument();
  });

  it("shows a reason holding markup as plain text", () => {
    show({ sessionId: first.id, at: "2026-09-28T15:10:00Z", reason: "<b>bold</b> <img src=x>", how: "stopped" });
    const line = screen.getByTestId("signed-out-ended");
    expect(line).toHaveTextContent("<b>bold</b> <img src=x>");
    expect(line.querySelector("b, img")).toBeNull();
  });
});

describe("the sign-out sheet's words and face", () => {
  it("shows the model with its effort, and the face of its worker", () => {
    show(undefined);
    const row = screen.getAllByTestId("signed-out-row")[0] as HTMLElement;
    expect(row).toHaveTextContent("Claude Code, claude-sonnet-5-5, medium effort, left 35 min ago");
    expect(within(row).getByTestId("worker-face")).toHaveAttribute("data-look", "nell-medium");
  });
});
