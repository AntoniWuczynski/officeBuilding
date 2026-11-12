import { describe, expect, it } from "vitest";
import { useEffect, useState } from "react";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockApi } from "../../api/mockApi";
import type { ChatCapabilities, ConversationBacklog, ConversationEntry, ConversationUpdate, QueuedNote, SessionState } from "../../types";
import { CLAUDE_TERMINAL, READ_ONLY, STRUCTURED, sampleTranscript } from "../chat/fixtures";
import { Conversation, ConversationApiContext, type ConversationApi } from "./Conversation";

/** A conversation the test controls: one backlog, and updates pushed by hand. */
class FakeApi implements ConversationApi {
  readonly prompts: string[] = [];
  readonly answers: string[] = [];
  private listener: ((u: ConversationUpdate) => void) | null = null;
  constructor(private readonly backlog: Partial<ConversationBacklog>) {}

  structuredBacklog(): Promise<ConversationBacklog> {
    return Promise.resolve({ transcript: [], state: "idle", approval: null, ended: null, capabilities: STRUCTURED, earlier: false, ...this.backlog });
  }
  subscribeStructured(_id: string, listener: (u: ConversationUpdate) => void): () => void {
    this.listener = listener;
    return () => {
      this.listener = null;
    };
  }
  structuredPrompt(_id: string, text: string): Promise<void> {
    this.prompts.push(text);
    return Promise.resolve();
  }
  structuredAnswer(_id: string, approvalId: string): Promise<void> {
    this.answers.push(approvalId);
    return Promise.resolve();
  }
  structuredCancel(): Promise<void> {
    return Promise.resolve();
  }
  structuredStop(): Promise<void> {
    return Promise.resolve();
  }
  withdrawMessage(): Promise<void> {
    return Promise.resolve();
  }
  push(entry: ConversationEntry | null, over: Partial<ConversationUpdate> = {}): void {
    act(() => {
      this.listener?.({ sessionId: "s", entry, state: "idle", approval: null, ended: null, reset: false, ...over });
    });
  }
}

function shown(api: FakeApi, onFlip?: () => void): ReturnType<typeof render> {
  return render(
    <ConversationApiContext value={api}>
      <Conversation sessionId="s" onFlip={onFlip} />
    </ConversationApiContext>,
  );
}

const MESSAGE_BOX = "Message to the agent";
const approval = { id: "ap", title: "Run it?", detail: "rm -rf nothing", fields: [], choices: [{ id: "ok", label: "Allow once", allow: true }] };
const message = (n: number): ConversationEntry => ({ kind: "agent", id: `m${n}`, text: `line ${n}` });

/** The conversation of a desk, with the waiting messages the way the floor passes them on. */
function Followed({ api, sessionId }: { readonly api: MockApi; readonly sessionId: string }): React.ReactElement {
  const [queued, setQueued] = useState<readonly QueuedNote[]>([]);
  useEffect(() => api.subscribe((snapshot) => setQueued(snapshot.queued.filter((n) => n.toSessionId === sessionId))), [api, sessionId]);
  return <Conversation sessionId={sessionId} queued={queued} />;
}

async function hired(task: string): Promise<{ readonly api: MockApi; readonly id: string }> {
  const api = new MockApi();
  const session = await api.spawnSession("p-arcade", { tool: "codex", title: task, model: "gpt-5.5", effort: "high", ultracode: false, view: "chat" });
  render(
    <ConversationApiContext value={api}>
      <Followed api={api} sessionId={session.id} />
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

  it("draws the same log under every capability set, and offers only what each allows", async () => {
    const logs: string[] = [];
    const sets: readonly [ChatCapabilities, { box: boolean; end: boolean }][] = [
      [STRUCTURED, { box: true, end: true }],
      [CLAUDE_TERMINAL, { box: true, end: false }],
      [READ_ONLY, { box: false, end: false }],
    ];
    for (const [capabilities, want] of sets) {
      const { container, unmount } = shown(new FakeApi({ transcript: sampleTranscript, capabilities }));
      await screen.findByText("Please tidy the notes and run the tests.");
      logs.push(container.querySelector(".conversation-log")?.innerHTML ?? "");
      expect(screen.queryByLabelText(MESSAGE_BOX) !== null).toBe(want.box);
      expect(screen.queryByRole("button", { name: "End agent" }) !== null).toBe(want.end);
      expect(screen.queryByText("This conversation is read-only here.") !== null).toBe(!want.box);
      unmount();
    }
    expect(logs[0]).not.toBe("");
    expect(logs[1]).toBe(logs[0]);
    expect(logs[2]).toBe(logs[0]);
  });

  it("offers Stop turn only while the desk is busy and the capability is there", async () => {
    const api = new FakeApi({ capabilities: CLAUDE_TERMINAL });
    shown(api);
    await screen.findByLabelText(MESSAGE_BOX);
    expect(screen.queryByRole("button", { name: "Stop turn" })).toBeNull();
    for (const state of ["thinking", "running"] as const satisfies readonly SessionState[]) {
      api.push(null, { state });
      expect(screen.getByRole("button", { name: "Stop turn" })).toBeInTheDocument();
    }
    api.push(null, { state: "idle" });
    expect(screen.queryByRole("button", { name: "Stop turn" })).toBeNull();
  });

  it("never offers Stop turn without the capability", async () => {
    shown(new FakeApi({ capabilities: { ...CLAUDE_TERMINAL, stop: false }, state: "running" }));
    await screen.findByLabelText(MESSAGE_BOX);
    expect(screen.queryByRole("button", { name: "Stop turn" })).toBeNull();
  });

  it("answers an approval in terminal mode by flipping, never by answering", async () => {
    const user = userEvent.setup();
    const api = new FakeApi({ capabilities: CLAUDE_TERMINAL, approval, state: "waiting-human" });
    let flips = 0;
    shown(api, () => {
      flips += 1;
    });
    const card = within(await screen.findByTestId("conversation-approval"));
    expect(card.getByText("Run it?")).toBeInTheDocument();
    expect(card.getByText("rm -rf nothing")).toBeInTheDocument();
    expect(card.queryByRole("button", { name: "Allow once" })).toBeNull();
    await user.click(card.getByRole("button", { name: "Answer in the terminal" }));
    expect(flips).toBe(1);
    expect(api.answers).toEqual([]);
  });

  it("has no button on an approval of a read-only conversation", async () => {
    shown(new FakeApi({ capabilities: READ_ONLY, approval }));
    const card = within(await screen.findByTestId("conversation-approval"));
    expect(card.queryByRole("button")).toBeNull();
    expect(card.getByText("Answer it where the session runs.")).toBeInTheDocument();
  });

  it("empties the log before adding the entry of a reset", async () => {
    const api = new FakeApi({ transcript: [message(1), message(2)] });
    shown(api);
    await screen.findByText("line 1");
    api.push(message(3), { reset: true });
    expect(screen.queryByText("line 1")).toBeNull();
    expect(screen.queryByText("line 2")).toBeNull();
    expect(screen.getByText("line 3")).toBeInTheDocument();
  });

  it("keeps 400 entries and says earlier ones are not shown", async () => {
    const api = new FakeApi({ transcript: [] });
    const { container } = shown(api);
    await screen.findByLabelText(MESSAGE_BOX);
    expect(screen.queryByText("Earlier messages are not shown here.")).toBeNull();
    for (let n = 1; n <= 401; n += 1) api.push(message(n));
    expect(container.querySelectorAll(".conversation-agent")).toHaveLength(400);
    expect(screen.queryByText("line 1")).toBeNull();
    expect(screen.getByText("line 401")).toBeInTheDocument();
    expect(screen.getByText("Earlier messages are not shown here.")).toBeInTheDocument();
  });

  it("shows the earlier-messages note from the start when the backlog says so", async () => {
    shown(new FakeApi({ transcript: [message(1)], earlier: true }));
    expect(await screen.findByText("Earlier messages are not shown here.")).toBeInTheDocument();
  });

  it("says nothing has been said in an empty conversation", async () => {
    shown(new FakeApi({}));
    expect(await screen.findByText("Nothing has been said yet.")).toBeInTheDocument();
  });

  it("sends a message while the desk is busy", async () => {
    const user = userEvent.setup();
    const api = new FakeApi({ state: "running" });
    shown(api);
    await user.type(await screen.findByLabelText(MESSAGE_BOX), "also this{Enter}");
    expect(api.prompts).toEqual(["also this"]);
  });

  it("draws a diff for a mock desk's edit", async () => {
    const user = userEvent.setup();
    await hired("");
    await user.type(await screen.findByLabelText(MESSAGE_BOX), "edit the notes{Enter}");
    expect(await screen.findByText("Edit notes.txt")).toBeInTheDocument();
    expect(document.querySelector(".chat-diff .is-add")?.textContent).toContain("new line");
  });

  it("keeps a message sent during an open turn in the queue, withdraws it, and sends one left alone when the turn ends", async () => {
    const user = userEvent.setup();
    await hired("");
    const box = await screen.findByLabelText(MESSAGE_BOX);
    await user.type(box, "run the tests{Enter}");
    const card = within(await screen.findByTestId("conversation-approval"));
    await user.type(box, "first extra{Enter}");
    await user.type(box, "second extra{Enter}");
    const queue = within(await screen.findByTestId("chat-queue"));
    expect(queue.getByText("first extra")).toBeInTheDocument();
    expect(screen.getByTestId("conversation").querySelector(".conversation-log")).not.toHaveTextContent("first extra");
    expect(queue.getAllByText("Waiting: it is waiting on you.")).toHaveLength(2);
    const second = queue.getByText("second extra").closest("li");
    expect(second).not.toBeNull();
    if (second === null) return;
    await user.click(within(second).getByRole("button", { name: "Withdraw" }));
    expect(screen.queryByText("second extra")).toBeNull();
    await user.click(card.getByRole("button", { name: "Allow once" }));
    expect(await screen.findByText("Done: first extra")).toBeInTheDocument();
    expect(screen.queryByTestId("chat-queue")).toBeNull();
    expect(screen.getByTestId("conversation").querySelector(".conversation-log")).toHaveTextContent("first extra");
  });
});
