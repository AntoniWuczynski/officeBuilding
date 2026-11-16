import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UsageSigns } from "./UsageSigns";
import { MockApi } from "../../api/mockApi";
import type { ToolUsage } from "../../types";

const load = (): Promise<readonly ToolUsage[]> => new MockApi().toolUsage();

describe("UsageSigns", () => {
  it("shows each window as a labelled meter with its reset and an as-of line", async () => {
    render(<UsageSigns loadUsage={load} />);
    const signs = await screen.findAllByTestId("usage-sign");
    const claude = within(signs[0] as HTMLElement);
    expect(claude.getByRole("heading", { name: "Claude Code" })).toBeInTheDocument();
    expect(claude.getByRole("meter", { name: "Session" })).toHaveAttribute("aria-valuenow", "8");
    expect(claude.getByRole("meter", { name: "Week · Fable" })).toHaveAttribute("aria-valuenow", "12");
    expect(claude.getByText("Resets in 3 h")).toBeInTheDocument();
    expect(claude.getByText("As of 20 min ago")).toBeInTheDocument();
  });

  it("colours meters by how full they are and omits an unknown reset", async () => {
    const usage: ToolUsage = {
      tool: "codex",
      checkedAt: new Date().toISOString(),
      unavailable: null,
      windows: [
        { id: "a", label: "Low", percent: 69, resetsAt: null },
        { id: "b", label: "Mid", percent: 70, resetsAt: null },
        { id: "c", label: "High", percent: 90, resetsAt: null },
      ],
    };
    render(<UsageSigns loadUsage={() => Promise.resolve([usage])} />);
    expect(await screen.findByRole("meter", { name: "Low" })).toHaveAttribute("data-level", "ok");
    expect(screen.getByRole("meter", { name: "Mid" })).toHaveAttribute("data-level", "wait");
    expect(screen.getByRole("meter", { name: "High" })).toHaveAttribute("data-level", "fault");
    expect(screen.queryByText(/Resets/)).not.toBeInTheDocument();
  });

  it("greys out an unavailable tool and gives its reason instead of meters", async () => {
    render(<UsageSigns loadUsage={load} />);
    const signs = await screen.findAllByTestId("usage-sign");
    const agy = signs[2] as HTMLElement;
    expect(agy).toHaveClass("is-unavailable");
    expect(agy).toHaveTextContent("Antigravity keeps its quota on Google's servers, not on this Mac");
    expect(within(agy).queryByRole("meter")).not.toBeInTheDocument();
  });

  it("asks for a fresh check when Re-check is pressed, and only then", async () => {
    const user = userEvent.setup();
    const loadUsage = vi.fn(load);
    render(<UsageSigns loadUsage={loadUsage} />);
    await screen.findAllByTestId("usage-sign");
    expect(loadUsage).toHaveBeenCalledTimes(1);
    expect(loadUsage).toHaveBeenLastCalledWith(false);
    await user.click(screen.getByRole("button", { name: "Re-check" }));
    await waitFor(() => expect(loadUsage).toHaveBeenCalledTimes(2));
    expect(loadUsage).toHaveBeenLastCalledWith(true);
    expect(await screen.findByRole("button", { name: "Re-check" })).toBeEnabled();
  });

  it("folds to one headline carrying the fullest window when collapsible, and opens and closes on request", async () => {
    const user = userEvent.setup();
    render(<UsageSigns loadUsage={load} collapsible />);
    const headline = await screen.findByRole("button", { name: "Claude Code Week 40%" });
    expect(headline).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("meter", { name: "Session" })).not.toBeInTheDocument();
    await user.click(headline);
    expect(screen.getByRole("meter", { name: "Session" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Re-check" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hide" }));
    expect(screen.getByRole("button", { name: "Claude Code Week 40%" })).toBeInTheDocument();
  });

  it("never folds outside a collapsible view, even if it was folded a moment ago", async () => {
    const { rerender } = render(<UsageSigns loadUsage={load} collapsible />);
    await screen.findByRole("button", { name: "Claude Code Week 40%" });
    rerender(<UsageSigns loadUsage={load} />);
    expect(screen.getByRole("meter", { name: "Session" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Hide" })).not.toBeInTheDocument();
  });
});
