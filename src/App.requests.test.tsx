import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";

const hire = { tool: "codex", title: "", model: "gpt-5.5", effort: "high", ultracode: false, view: "chat" } as const;

/** A floor with a chat desk that was asked to do something, the floor open. */
async function floorWith(prompt: string): Promise<{ api: MockApi; user: ReturnType<typeof userEvent.setup> }> {
  const user = userEvent.setup();
  const api = new MockApi();
  const desk = await api.spawnSession("p-arcade", hire);
  await api.structuredPrompt(desk.id, prompt);
  render(<App api={api} />);
  await user.click((await screen.findAllByTestId("storey")).find((s) => s.getAttribute("data-project") === "p-arcade") as HTMLElement);
  await screen.findByTestId("back-button");
  return { api, user };
}
const decisionRows = (): HTMLElement[] => within(screen.getByTestId("sidebar")).queryAllByTestId("question-row");

describe("a chat desk's request, from the queue to its card", () => {
  it("lists the request in Decisions and in Needs you, and opens its card from either", async () => {
    const { user } = await floorWith("ask me");
    await user.click(screen.getByTestId("rail-decisions"));
    expect(decisionRows()).toHaveLength(1);
    expect(decisionRows()[0]).toHaveTextContent("answered in its conversation");
    await user.click(decisionRows()[0] as HTMLElement);
    expect(screen.queryByTestId("question-detail")).toBeNull();
    const card = await screen.findByTestId("conversation-approval");
    expect(card).toHaveAttribute("data-focused", "true");
    await user.selectOptions(within(card).getByLabelText("Database (needed)"), "SQLite");
    await user.click(within(card).getByRole("button", { name: "Send" }));
    await waitFor(() => expect(decisionRows()).toHaveLength(0));
    expect(screen.queryByTestId("conversation-approval")).toBeNull();
  });

  it("opens the card from Needs you with its Answer button", async () => {
    const { user } = await floorWith("run it");
    await user.click(screen.getByTestId("back-button"));
    await user.click(await screen.findByTestId("needs-you"));
    const own = screen.getAllByTestId("needs-you-row").filter((r) => (r.getAttribute("data-question") ?? "").includes(":request:"));
    expect(own).toHaveLength(1);
    const row = within(own[0] as HTMLElement);
    await user.click(row.getByRole("button", { name: "Answer" }));
    expect(await screen.findByTestId("conversation-approval")).toHaveAttribute("data-focused", "true");
  });

  it("keeps the first request when the second is answered", async () => {
    const { user } = await floorWith("ask and run");
    await user.click(screen.getByTestId("rail-decisions"));
    expect(decisionRows()).toHaveLength(2);
    // The blocking command outranks the background question, so it is the first row.
    await user.click(decisionRows()[0] as HTMLElement);
    const cards = await screen.findAllByTestId("conversation-approval");
    expect(cards).toHaveLength(2);
    expect(cards[1]).toHaveAttribute("data-focused", "true");
    await user.click(within(cards[1] as HTMLElement).getByRole("button", { name: "Decline" }));
    await waitFor(() => expect(screen.getAllByTestId("conversation-approval")).toHaveLength(1));
    expect(decisionRows()).toHaveLength(1);
  });
});
