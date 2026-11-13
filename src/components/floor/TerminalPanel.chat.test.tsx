import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TerminalApiContext, TerminalPanel } from "./TerminalPanel";
import type { TerminalApi } from "./TerminalPanel";
import { ConversationApiContext } from "./Conversation";
import type { ConversationApi } from "./Conversation";
import { makeSeedSnapshot } from "../../api/mockData";
import type { ConversationBacklog, QueuedNote, Session } from "../../types";
import { CLAUDE_TERMINAL, STRUCTURED } from "../chat/fixtures";


const terminalApi: TerminalApi = {
  subscribeTerminal: () => () => {},
  writeTerminal: () => Promise.resolve(),
  resizeTerminal: () => Promise.resolve(),
  subscribeFileDrops: () => () => {},
};

const answered = vi.fn();
const backlog = (over: Partial<ConversationBacklog> = {}): ConversationBacklog => ({
  transcript: [],
  state: "idle",
  approval: null,
  ended: null,
  capabilities: CLAUDE_TERMINAL,
  earlier: false,
  ...over,
});
let next: ConversationBacklog = backlog();
const conversationApi: ConversationApi = {
  structuredBacklog: () => Promise.resolve(next),
  subscribeStructured: () => () => {},
  structuredPrompt: () => Promise.resolve(),
  structuredAnswer: (...args) => {
    answered(...args);
    return Promise.resolve();
  },
  structuredCancel: () => Promise.resolve(),
  structuredStop: () => Promise.resolve(),
  withdrawMessage: () => Promise.resolve(),
};

const seed = makeSeedSnapshot().sessions[0];
if (seed === undefined) throw new Error("the seed snapshot has no session");
const base: Session = seed;
const claude = (id: string, over: Partial<Session> = {}): Session => ({ ...base, id, tool: "claude-code", control: "full", ...over });

function show(session: Session, props: { readonly chat?: boolean; readonly queued?: readonly QueuedNote[] } = {}): ReturnType<typeof render> {
  return render(
    <TerminalApiContext value={terminalApi}>
      <ConversationApiContext value={conversationApi}>
        <TerminalPanel session={session} onClose={() => {}} {...props} />
      </ConversationApiContext>
    </TerminalApiContext>,
  );
}

const pressed = (name: string): boolean => screen.getByRole("button", { name }).getAttribute("aria-pressed") === "true";

beforeEach(() => {
  localStorage.clear();
  answered.mockClear();
  next = backlog();
});

describe("TerminalPanel flipping", () => {
  it("opens a Claude Code desk in the chat view when its id is in the chat desks", async () => {
    show(claude("flip-1"), { chat: true });
    expect(await screen.findByTestId("conversation")).toBeInTheDocument();
    expect(pressed("Chat")).toBe(true);
    expect(pressed("Terminal")).toBe(false);
  });

  it("opens on the terminal without it, and a click on Chat shows the conversation", async () => {
    const user = userEvent.setup();
    show(claude("flip-2"));
    expect(screen.getByTestId("terminal-screen")).toBeInTheDocument();
    expect(pressed("Terminal")).toBe(true);
    await user.click(screen.getByRole("button", { name: "Chat" }));
    expect(await screen.findByTestId("conversation")).toBeInTheDocument();
    expect(screen.queryByTestId("terminal-screen")).toBeNull();
  });

  it("flips back to the terminal, and remembers the choice when the panel is opened again", async () => {
    const user = userEvent.setup();
    const first = show(claude("flip-3"), { chat: true });
    await user.click(await screen.findByRole("button", { name: "Terminal" }));
    expect(screen.getByTestId("terminal-screen")).toBeInTheDocument();
    first.unmount();
    show(claude("flip-3"), { chat: true });
    expect(screen.getByTestId("terminal-screen")).toBeInTheDocument();
    expect(pressed("Terminal")).toBe(true);
  });

  it("has no switch on a structured desk or a Codex terminal desk", async () => {
    next = backlog({ capabilities: STRUCTURED });
    const structured = show(claude("flip-4", { tool: "codex", control: "structured" }));
    expect(await screen.findByTestId("conversation")).toBeInTheDocument();
    expect(screen.queryByTestId("view-switch")).toBeNull();
    structured.unmount();
    show(claude("flip-5", { tool: "codex" }));
    expect(screen.getByTestId("terminal-screen")).toBeInTheDocument();
    expect(screen.queryByTestId("view-switch")).toBeNull();
  });

  it("flips to the terminal from the permission card and sends nothing", async () => {
    const user = userEvent.setup();
    next = backlog({
      state: "waiting-human",
      approval: { id: "terminal-prompt", title: "Claude Code is waiting for your answer", detail: "", fields: [], choices: [] },
    });
    show(claude("flip-6"), { chat: true });
    await user.click(await screen.findByRole("button", { name: "Answer in the terminal" }));
    expect(screen.getByTestId("terminal-screen")).toBeInTheDocument();
    expect(answered).not.toHaveBeenCalled();
  });

  it("labels the panel by what it shows", async () => {
    const chat = show(claude("flip-7"), { chat: true });
    expect(await screen.findByLabelText(/^Conversation: /)).toBeInTheDocument();
    chat.unmount();
    show(claude("flip-8"));
    expect(screen.getByLabelText(/^Terminal: /)).toBeInTheDocument();
  });

  it("shows the waiting messages in the composer", async () => {
    const queued: QueuedNote[] = [{ id: "q-1", toSessionId: "flip-9", text: "when you can", queuedAt: "2026-09-28T15:40:00Z", hold: "busy" }];
    show(claude("flip-9"), { chat: true, queued });
    expect(within(await screen.findByTestId("chat-queue")).getByText("when you can")).toBeInTheDocument();
  });
});

describe("TerminalPanel height", () => {
  it("opens a chat at 60% and a terminal at 260px", async () => {
    const chat = show(claude("tall-1"), { chat: true });
    expect(screen.getByTestId("terminal-panel").style.height).toBe("60%");
    chat.unmount();
    show(claude("tall-2"));
    expect(screen.getByTestId("terminal-panel").style.height).toBe("260px");
  });

  it("keeps a chat's height under its own key and leaves the terminal's alone", async () => {
    show(claude("tall-3"), { chat: true });
    const terminalKey = localStorage.getItem("office-building:terminal-height");
    expect(localStorage.getItem("office-building:chat-height")).toBeNull();
    fireEvent.keyDown(screen.getByTestId("terminal-resize"), { key: "ArrowUp" });
    expect(Number(localStorage.getItem("office-building:chat-height"))).toBeGreaterThanOrEqual(140);
    expect(localStorage.getItem("office-building:terminal-height")).toBe(terminalKey);
  });

  it("opens a chat at the height it was left at", () => {
    localStorage.setItem("office-building:chat-height", "333");
    show(claude("tall-4"), { chat: true });
    expect(screen.getByTestId("terminal-panel").style.height).toBe("333px");
  });
});
