import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";

async function enterInfo(projectId: string): Promise<void> {
  const user = userEvent.setup();
  const storeys = await screen.findAllByTestId("storey");
  const storey = storeys.find((s) => s.getAttribute("data-project") === projectId);
  if (storey === undefined) throw new Error(`no storey ${projectId}`);
  await user.click(within(storey).getByTestId("floor-nameplate"));
  await screen.findByTestId("back-button");
  await user.click(screen.getByTestId("rail-info"));
}

describe("QA from the Info panel", () => {
  it("switches a floor on after confirming, and says so", async () => {
    const api = new MockApi();
    const spy = vi.spyOn(api, "setFloorQa");
    render(<App api={api} />);
    await enterInfo("p-notes");
    await userEvent.click(screen.getByRole("switch", { name: "Review my pull requests" }));
    await userEvent.click(screen.getByTestId("qa-confirm-yes"));
    expect(spy).toHaveBeenCalledWith("p-notes", true);
    expect(await screen.findByTestId("toast")).toHaveTextContent("QA is on for this floor. The floor is pinned.");
    expect(await screen.findByTestId("qa-status")).toHaveTextContent("Asking GitHub");
  });

  it("toasts a Check now that fails, and one that opens a review that fails", async () => {
    const api = new MockApi();
    vi.spyOn(api, "recheckQa").mockRejectedValue(new Error("gh is not installed"));
    vi.spyOn(api, "openReview").mockRejectedValue(new Error("no browser"));
    render(<App api={api} />);
    await enterInfo("p-shop");
    await userEvent.click(screen.getByTestId("qa-recheck"));
    expect(await screen.findByText("Could not ask GitHub: gh is not installed.")).toBeInTheDocument();
    const [review] = screen.getAllByTestId("qa-review");
    if (review === undefined) throw new Error("no review listed");
    await userEvent.click(review);
    expect(await screen.findByText("Could not open that review: no browser.")).toBeInTheDocument();
  });
});
