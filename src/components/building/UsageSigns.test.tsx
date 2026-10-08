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

  it("reads the usage again when Re-check is pressed", async () => {
    const user = userEvent.setup();
    const loadUsage = vi.fn(load);
    render(<UsageSigns loadUsage={loadUsage} />);
    await screen.findAllByTestId("usage-sign");
    expect(loadUsage).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Re-check" }));
    await waitFor(() => expect(loadUsage).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole("button", { name: "Re-check" })).toBeEnabled();
  });
});
