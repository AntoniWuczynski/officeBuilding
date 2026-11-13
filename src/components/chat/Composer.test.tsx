import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ChatCapabilities, NoteHold, QueuedNote } from "../../types";
import { CLAUDE_TERMINAL, STRUCTURED } from "./fixtures";
import { Composer } from "./Composer";

const note = (id: string, text: string, hold: NoteHold = "busy"): QueuedNote => ({ id, toSessionId: "s", text, queuedAt: "2026-09-28T15:40:00Z", hold });

function shown(over: { capabilities?: ChatCapabilities; busy?: boolean; queued?: readonly QueuedNote[] } = {}) {
  const calls = { onSend: vi.fn<(t: string) => Promise<void>>(() => Promise.resolve()), onWithdraw: vi.fn<(id: string) => void>(), onStop: vi.fn(), onEnd: vi.fn() };
  render(<Composer capabilities={STRUCTURED} busy={false} queued={[]} {...calls} {...over} />);
  return calls;
}

describe("Composer queue", () => {
  it("draws no list when nothing waits", () => {
    shown();
    expect(screen.queryByTestId("chat-queue")).toBeNull();
  });

  it("lists waiting messages in order, each with its reason and a Withdraw button", async () => {
    const user = userEvent.setup();
    const { onWithdraw } = shown({ queued: [note("q-1", "first"), note("q-2", "second")] });
    const rows = within(screen.getByTestId("chat-queue")).getAllByRole("listitem");
    expect(rows.map((r) => r.querySelector(".chat-queue-text")?.textContent)).toEqual(["first", "second"]);
    for (const r of rows) expect(r).toHaveTextContent("Waiting: it is busy.");
    const second = rows[1];
    expect(second).toBeDefined();
    if (second === undefined) return;
    await user.click(within(second).getByRole("button", { name: "Withdraw" }));
    expect(onWithdraw).toHaveBeenCalledWith("q-2");
  });

  it("keeps the whole text in the title, since the row shows three lines", () => {
    shown({ queued: [note("q-1", "a\nb\nc\nd")] });
    expect(document.querySelector(".chat-queue-text")).toHaveAttribute("title", "a\nb\nc\nd");
  });

  it("gives the three reasons of your own messages", () => {
    shown({ queued: [note("q-1", "a", "you-typed"), note("q-2", "b", "awaiting-turn"), note("q-3", "c", "no-idle-signal")] });
    const list = screen.getByTestId("chat-queue");
    expect(list).toHaveTextContent("Waiting: its terminal holds something you typed and have not sent.");
    expect(list).toHaveTextContent("Waiting: it has not picked up your last message yet.");
    expect(list).toHaveTextContent("Waiting: it has not told the building it is idle yet, which can take a minute after it starts.");
  });
});

describe("Composer box", () => {
  it("sends on Enter and clears the box, and adds a line on Shift+Enter", async () => {
    const user = userEvent.setup();
    const { onSend } = shown();
    const box = screen.getByLabelText("Message to the agent");
    await user.type(box, "one{Shift>}{Enter}{/Shift}two");
    expect(box).toHaveValue("one\ntwo");
    expect(onSend).not.toHaveBeenCalled();
    await user.type(box, "{Enter}");
    expect(onSend).toHaveBeenCalledWith("one\ntwo");
    expect(box).toHaveValue("");
  });

  it("keeps the text when the send is refused", async () => {
    const user = userEvent.setup();
    const { onSend } = shown();
    onSend.mockRejectedValueOnce(new Error("too long"));
    const box = screen.getByLabelText("Message to the agent");
    await user.type(box, "hello{Enter}");
    expect(onSend).toHaveBeenCalledWith("hello");
    expect(box).toHaveValue("hello");
  });

  it("sends once while a send is in flight, then sends again once it settles", async () => {
    const user = userEvent.setup();
    const { onSend } = shown();
    let settle = (): void => undefined;
    onSend.mockImplementationOnce(() => new Promise<void>((resolve) => (settle = resolve)));
    const box = screen.getByLabelText("Message to the agent");
    await user.type(box, "hello{Enter}{Enter}");
    await user.click(screen.getByRole("button", { name: "Send" }));
    expect(onSend).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    settle();
    await waitFor(() => expect(box).toHaveValue(""));
    await user.type(box, "again{Enter}");
    expect(onSend).toHaveBeenCalledTimes(2);
  });

  it("sends nothing from an empty box", async () => {
    const user = userEvent.setup();
    const { onSend } = shown();
    await user.type(screen.getByLabelText("Message to the agent"), "   {Enter}");
    expect(onSend).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
  });

  it("offers Stop turn only when busy with the capability, and End agent only with the capability", async () => {
    const user = userEvent.setup();
    const idle = shown();
    expect(screen.queryByRole("button", { name: "Stop turn" })).toBeNull();
    expect(screen.getByRole("button", { name: "End agent" })).toBeInTheDocument();
    idle.onEnd.mockClear();
    await user.click(screen.getByRole("button", { name: "End agent" }));
    expect(idle.onEnd).toHaveBeenCalledTimes(1);
  });

  it("stops a busy turn, and has no End agent for Claude Code in a terminal", async () => {
    const user = userEvent.setup();
    const { onStop } = shown({ capabilities: CLAUDE_TERMINAL, busy: true });
    expect(screen.queryByRole("button", { name: "End agent" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Stop turn" }));
    expect(onStop).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText("Message to the agent")).toHaveAttribute("placeholder", "The agent is working. Your message waits until it is idle.");
  });
});
