import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockApi } from "../../api/mockApi";
import { Conversation, ConversationApiContext } from "./Conversation";

async function hired(task: string): Promise<{ readonly api: MockApi; readonly id: string }> {
  const api = new MockApi();
  const session = await api.spawnSession("p-arcade", { tool: "codex", title: task, model: "gpt-5.5", effort: "high", ultracode: false, structured: true });
  render(
    <ConversationApiContext value={api}>
      <Conversation sessionId={session.id} />
    </ConversationApiContext>,
  );
  return { api, id: session.id };
}

describe("Conversation", () => {
  it("shows the first task and the agent's answer, and sends the next message", async () => {
    const user = userEvent.setup();
    const { api, id } = await hired("tidy the README");
    expect(await screen.findByText("tidy the README")).toBeInTheDocument();
    expect(screen.getByText("Done: tidy the README")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Message to the agent"), "now the changelog{Enter}");
    expect(await screen.findByText("Done: now the changelog")).toBeInTheDocument();
    expect((await api.structuredBacklog(id)).transcript).toHaveLength(4);
  });

  it("asks before running something and goes on once allowed", async () => {
    const user = userEvent.setup();
    const { api } = await hired("");
    await user.type(await screen.findByLabelText("Message to the agent"), "run the tests{Enter}");
    const approval = within(await screen.findByTestId("conversation-approval"));
    expect(approval.getByText("echo hi")).toBeInTheDocument();
    const desk = (await api.getSnapshot()).sessions.find((s) => s.control === "structured");
    expect(desk?.state).toBe("waiting-human");
    await user.click(approval.getByRole("button", { name: "Allow once" }));
    expect(await screen.findByText("It printed hi.")).toBeInTheDocument();
    expect(screen.queryByTestId("conversation-approval")).toBeNull();
  });

  it("says when the agent has ended and takes no more messages", async () => {
    const user = userEvent.setup();
    const { api } = await hired("");
    await user.click(await screen.findByRole("button", { name: "End agent" }));
    expect(await screen.findByText(/The agent has finished/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Message to the agent")).toBeNull();
    expect((await api.getSnapshot()).sessions.some((s) => s.control === "structured")).toBe(false);
  });

  it("fills in a question's field, with an answer of its own, before sending", async () => {
    const user = userEvent.setup();
    await hired("");
    await user.type(await screen.findByLabelText("Message to the agent"), "ask me{Enter}");
    const card = within(await screen.findByTestId("conversation-approval"));
    expect(card.getByRole("button", { name: "Send" })).toBeDisabled();
    await user.selectOptions(card.getByLabelText("Database (needed)"), "Something else…");
    await user.type(card.getByLabelText("Database: your own answer"), "MySQL");
    await user.click(card.getByRole("button", { name: "Send" }));
    expect(await screen.findByText("Going with MySQL.")).toBeInTheDocument();
  });
});
