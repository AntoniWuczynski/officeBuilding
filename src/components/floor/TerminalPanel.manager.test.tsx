import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { TerminalApiContext, TerminalPanel } from "./TerminalPanel";
import type { TerminalApi } from "./TerminalPanel";
import { makeSeedSnapshot } from "../../api/mockData";
import type { ManagerFloor, Session } from "../../types";
import { ConversationApiContext } from "./Conversation";
import type { ConversationApi } from "./Conversation";
import type { ConversationBacklog } from "../../types";
import { CLAUDE_TERMINAL } from "../chat/fixtures";

const backlog: ConversationBacklog = { transcript: [], state: "idle", approvals: [], ended: null, capabilities: CLAUDE_TERMINAL, earlier: false };
const conversationApi: ConversationApi = {
  structuredBacklog: () => Promise.resolve(backlog),
  subscribeStructured: () => () => {},
  structuredPrompt: () => Promise.resolve(),
  structuredAnswer: () => Promise.resolve(),
  structuredCancel: () => Promise.resolve(),
  structuredStop: () => Promise.resolve(),
  withdrawMessage: () => Promise.resolve(),
};


const api: TerminalApi = {
  subscribeTerminal: () => () => {},
  writeTerminal: () => Promise.resolve(),
  resizeTerminal: () => Promise.resolve(),
  subscribeFileDrops: () => () => {},
};

function pieces(): { readonly session: Session; readonly manager: ManagerFloor } {
  const seed = makeSeedSnapshot();
  const session = seed.sessions.find((s) => s.id === "s-1");
  const manager = seed.managers.find((m) => m.projectId === "p-shop");
  if (session === undefined || manager === undefined) throw new Error("seed is missing s-1 or the shop's manager");
  return { session, manager };
}

function show(session: Session, manager: ManagerFloor | undefined): void {
  render(
    <TerminalApiContext value={api}>
      <TerminalPanel session={session} manager={manager} onClose={() => {}} />
    </TerminalApiContext>,
  );
}

describe("TerminalPanel for a managed desk", () => {
  it("shows the managed mark and the notes that wait", () => {
    const { session, manager } = pieces();
    show(session, manager);
    expect(screen.getByTestId("terminal-managed")).toHaveTextContent("managed");
    expect(screen.getByTestId("terminal-notes")).toHaveTextContent("1 note from the manager waiting");
  });

  it("says nothing for a desk the manager does not drive", () => {
    const { session, manager } = pieces();
    show(session, { ...manager, managed: [], queue: [] });
    expect(screen.queryByTestId("terminal-managed")).not.toBeInTheDocument();
    expect(screen.queryByTestId("terminal-notes")).not.toBeInTheDocument();
  });

  it("says nothing without a manager", () => {
    show(pieces().session, undefined);
    expect(screen.queryByTestId("terminal-managed")).not.toBeInTheDocument();
  });

  it("shows the waiting note's strip, and none for a desk without notes", () => {
    const { session, manager } = pieces();
    show(session, manager);
    expect(screen.getByTestId("manager-note-strip")).toHaveTextContent("When the checkout audit is done");
  });

  it("shows no strip when no note waits for the desk", () => {
    const { session, manager } = pieces();
    show(session, { ...manager, queue: [] });
    expect(screen.queryByTestId("manager-note-strip")).not.toBeInTheDocument();
  });

  it("shows no strip for a shell", () => {
    const { manager } = pieces();
    render(
      <TerminalApiContext value={api}>
        <TerminalPanel shell={{ id: "sh-1", projectId: "p-shop", number: 1, startedAt: "2026-09-28T15:00:00Z" }} path="/tmp" manager={manager} onClose={() => {}} />
      </TerminalApiContext>,
    );
    expect(screen.queryByTestId("manager-note-strip")).not.toBeInTheDocument();
  });

  it("shows the strip in the chat face too", async () => {
    const { session, manager } = pieces();
    render(
      <TerminalApiContext value={api}>
        <ConversationApiContext value={conversationApi}>
          <TerminalPanel session={session} manager={manager} chat onClose={() => {}} />
        </ConversationApiContext>
      </TerminalApiContext>,
    );
    expect(await screen.findByTestId("conversation")).toBeInTheDocument();
    expect(screen.getByTestId("manager-note-strip")).toBeInTheDocument();
  });
});
