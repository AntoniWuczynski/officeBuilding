import { afterEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UsageSigns } from "./UsageSigns";
import { MockApi } from "../../api/mockApi";
import type { ToolUsage } from "../../types";

const load = (): Promise<readonly ToolUsage[]> => new MockApi().toolUsage();
const office = { name: "office", startOpen: true } as const;
const islands = { name: "islands", startOpen: false } as const;

afterEach(() => localStorage.clear());

describe("UsageSigns in both views", () => {
  it("starts open in the office and folded on the islands, as each did before", async () => {
    const { unmount } = render(<UsageSigns loadUsage={load} collapsible view={office} />);
    expect(await screen.findByRole("meter", { name: "Session" })).toBeInTheDocument();
    unmount();
    render(<UsageSigns loadUsage={load} collapsible view={islands} />);
    expect(await screen.findByRole("button", { name: "Claude Code Week 40%" })).toBeInTheDocument();
  });

  it("lets the office fold them, and each view keeps its own last choice, kept across a reload", async () => {
    const user = userEvent.setup();
    const { rerender, unmount } = render(<UsageSigns loadUsage={load} collapsible view={office} />);
    await user.click(await screen.findByRole("button", { name: "Hide" }));
    expect(screen.getByRole("button", { name: "Claude Code Week 40%" })).toBeInTheDocument();
    // The islands' own choice is untouched, and the office's is still folded when the view comes back.
    rerender(<UsageSigns loadUsage={load} collapsible view={islands} />);
    expect(screen.getByRole("button", { name: "Claude Code Week 40%" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Claude Code Week 40%" }));
    expect(screen.getByRole("meter", { name: "Session" })).toBeInTheDocument();
    rerender(<UsageSigns loadUsage={load} collapsible view={office} />);
    expect(screen.getByRole("button", { name: "Claude Code Week 40%" })).toBeInTheDocument();
    unmount();
    render(<UsageSigns loadUsage={load} collapsible view={islands} />);
    expect(await screen.findByRole("meter", { name: "Session" })).toBeInTheDocument();
  });
});
