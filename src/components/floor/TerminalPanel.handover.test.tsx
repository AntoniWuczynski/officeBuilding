import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeSeedSnapshot } from "../../api/mockData";
import { MockHandover, makeHandoverView } from "../../api/mockHandover";
import { makeHireOptions } from "../../api/mockHireOptions";
import { HandoverContext } from "../handover/HandoverContext";
import { EditorContext } from "../editor/EditorContext";
import { boundTo, floor, withManager } from "./floorHarness";
import { TerminalApiContext, TerminalPanel } from "./TerminalPanel";
import type { TerminalApi } from "./TerminalPanel";
import { ConversationApiContext } from "./Conversation";
import type { ConversationApi } from "./Conversation";
import { CLAUDE_TERMINAL } from "../chat/fixtures";
import type { ConversationBacklog, DeskHandoverState, OfficeSnapshot, Session, TakeOver } from "../../types";

const backlog: ConversationBacklog = { transcript: [], state: "idle", approval: null, ended: null, capabilities: CLAUDE_TERMINAL, earlier: false };
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

function withHandover(sessionId: string, state: DeskHandoverState) {
  return { ...makeSeedSnapshot(), handover: makeHandoverView([{ sessionId, state }]) };
}

describe("the handover on the desk", () => {
  it("shows the strip in the docked panel for a session that has one", async () => {
    const session = makeSeedSnapshot().sessions.find((s) => s.id === "s-1");
    if (session === undefined) throw new Error("seed is missing s-1");
    const state: DeskHandoverState = { kind: "underway", to: "codex" };
    render(
      <TerminalApiContext value={api}>
        <TerminalPanel session={session} manager={undefined} handover={state} onClose={() => {}} />
      </TerminalApiContext>,
    );
    expect(await screen.findByTestId("handover-strip")).toHaveTextContent("Handing this desk over to Codex.");
  });

  it("marks a worker that arrived by handover", () => {
    render(floor(withHandover("s-1", { kind: "arrived", from: "claude-code", at: 0, window: null, resetAt: null }), "p-shop"));
    const worker = screen.getAllByTestId("worker").find((w) => w.dataset.session === "s-1");
    expect(worker).toHaveTextContent("from Claude Code");
    expect(worker).toHaveAccessibleName(/Handed over from Claude Code at/);
  });

  it("shows a left desk's line on the sign-out sheet", () => {
    render(floor(withHandover("e-1", { kind: "left", to: "codex", at: 0 }), "p-shop", { section: "signed-out" }));
    const rows = screen.getAllByTestId("signed-out-row");
    expect(within(rows[0] ?? document.body).getByTestId("signed-out-handover")).toHaveTextContent("Handed over to Codex at");
    expect(within(rows[1] ?? document.body).queryByTestId("signed-out-handover")).not.toBeInTheDocument();
  });
});

const OVER: TakeOver = { id: "h-1", fromSession: "s-2", from: "codex", to: "claude-code", toSession: "s-4", at: 1_800_000_000 };

function snapshotWith(change: (s: Session) => Session, takeOvers: readonly TakeOver[] = []): OfficeSnapshot {
  const seed = makeSeedSnapshot();
  return { ...seed, sessions: seed.sessions.map(change), handover: { ...makeHandoverView(), takeOvers } };
}

/** The shop floor with `sessionId`'s panel open and a handover provider, or none. */
function open(snapshot: OfficeSnapshot, sessionId: string | null, handover: MockHandover | null = new MockHandover()) {
  const loadHireOptions = () => Promise.resolve(makeHireOptions());
  const el = floor(snapshot, "p-shop", { openSessionId: sessionId, loadHireOptions });
  return render(
    <HandoverContext value={handover}>
      <EditorContext value={{ list: { editors: [], defaultEditor: null }, open: vi.fn() }}>
        <TerminalApiContext value={api}>
          <ConversationApiContext value={conversationApi}>{el}</ConversationApiContext>
        </TerminalApiContext>
      </EditorContext>
    </HandoverContext>,
  );
}

const same = (s: Session): Session => s;
const joinShop = (s: Session): Session => (s.id === "s-4" ? { ...s, projectId: "p-shop" } : s);

describe("the take-over button on the panel", () => {
  it("is on the panel of a Codex desk and opens the sheet in take-over mode", async () => {
    const user = userEvent.setup();
    open(snapshotWith(same), "s-2");
    await user.click(await screen.findByTestId("take-over-button"));
    expect(await screen.findByRole("heading", { name: /Take over “orders ingest fix” with a new session/ })).toBeInTheDocument();
    expect(screen.getByTestId("take-over-confirm")).toBeInTheDocument();
  });

  it("starts the take-over with that session's id, then closes the sheet", async () => {
    const user = userEvent.setup();
    const handover = new MockHandover();
    open(snapshotWith(same), "s-2", handover);
    await user.click(await screen.findByTestId("take-over-button"));
    await user.click(await screen.findByLabelText("Claude Code"));
    await user.selectOptions(screen.getByTestId("hire-model"), "claude-opus-5-5");
    await user.click(screen.getByLabelText("High"));
    await user.click(screen.getByTestId("hire-submit"));
    await waitFor(() => expect(screen.queryByTestId("hire-dialog")).not.toBeInTheDocument());
    expect(handover.taken).toEqual([{ sessionId: "s-2", start: { tool: "claude-code", model: "claude-opus-5-5", effort: "high" } }]);
  });

  it("keeps the sheet open and says why when the start is refused", async () => {
    const user = userEvent.setup();
    const handover = new MockHandover();
    handover.failNext("that session is no longer at a desk");
    open(snapshotWith(same), "s-2", handover);
    await user.click(await screen.findByTestId("take-over-button"));
    await user.click(await screen.findByLabelText("Claude Code"));
    await user.selectOptions(screen.getByTestId("hire-model"), "claude-opus-5-5");
    await user.click(screen.getByLabelText("High"));
    await user.click(screen.getByTestId("hire-submit"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not take over: that session is no longer at a desk.");
    expect(screen.getByTestId("hire-dialog")).toBeInTheDocument();
  });

  it("is missing for the reviewer, the manager, a structured desk and an opencode desk", async () => {
    // s-1 is the shop's reviewer in the seed.
    const cases: readonly [string, OfficeSnapshot][] = [
      ["s-1", snapshotWith(same)],
      ["s-2", withManager(snapshotWith(same), "p-shop", boundTo("s-2"))],
      ["s-2", snapshotWith((s) => (s.id === "s-2" ? { ...s, control: "structured" } : s))],
      ["s-3", snapshotWith(same)],
    ];
    for (const [id, snapshot] of cases) {
      const view = open(snapshot, id);
      await screen.findByTestId("terminal-panel");
      expect(screen.queryByTestId("take-over-button"), id).not.toBeInTheDocument();
      view.unmount();
    }
  });

  it("is missing for everybody with no handover provider", async () => {
    open(snapshotWith(same), "s-2", null);
    await screen.findByTestId("terminal-panel");
    expect(screen.queryByTestId("take-over-button")).not.toBeInTheDocument();
  });

  it("closes an open Hire sheet when the take-over sheet opens", async () => {
    const user = userEvent.setup();
    open(snapshotWith(same), "s-2");
    await user.click(await screen.findByTestId("hire-desk"));
    expect(await screen.findByRole("heading", { name: /Hire an agent/ })).toBeInTheDocument();
    await user.click(screen.getByTestId("take-over-button"));
    expect(await screen.findByRole("heading", { name: /Take over/ })).toBeInTheDocument();
    expect(screen.getAllByTestId("hire-dialog")).toHaveLength(1);
    expect(screen.queryByRole("heading", { name: /Hire an agent/ })).not.toBeInTheDocument();
  });

  it("closes the take-over sheet when the Hire sheet opens", async () => {
    const user = userEvent.setup();
    open(snapshotWith(same), "s-2");
    await user.click(await screen.findByTestId("take-over-button"));
    await screen.findByRole("heading", { name: /Take over/ });
    await user.click(screen.getByTestId("hire-desk"));
    expect(await screen.findByRole("heading", { name: /Hire an agent/ })).toBeInTheDocument();
    expect(screen.getAllByTestId("hire-dialog")).toHaveLength(1);
  });
});

describe("the worker's right-click menu", () => {
  const worker = (id: string): HTMLElement => {
    const w = screen.getAllByTestId("worker").find((x) => x.dataset.session === id);
    if (w === undefined) throw new Error(`no worker ${id}`);
    return w;
  };

  it("has the extra line for a desk that may be taken over, and picking it opens the same sheet", async () => {
    const user = userEvent.setup();
    open(snapshotWith(same), null);
    await user.pointer({ keys: "[MouseRight]", target: worker("s-2") });
    await user.click(await screen.findByTestId("menu-extra"));
    expect(await screen.findByRole("heading", { name: /Take over “orders ingest fix”/ })).toBeInTheDocument();
    expect(screen.queryByTestId("editor-menu")).not.toBeInTheDocument();
  });

  it("has no extra line for an opencode desk or with no provider", async () => {
    const user = userEvent.setup();
    const view = open(snapshotWith(same), null);
    await user.pointer({ keys: "[MouseRight]", target: worker("s-3") });
    await screen.findByTestId("editor-menu");
    expect(screen.queryByTestId("menu-extra")).not.toBeInTheDocument();
    view.unmount();
    open(snapshotWith(same), null, null);
    await user.pointer({ keys: "[MouseRight]", target: worker("s-2") });
    await screen.findByTestId("editor-menu");
    expect(screen.queryByTestId("menu-extra")).not.toBeInTheDocument();
  });
});

describe("the note of a take-over", () => {
  it("shows in the panel of the old and the new session, and on the old one's hover card", async () => {
    const user = userEvent.setup();
    const view = open(snapshotWith(joinShop, [OVER]), "s-2");
    expect(await screen.findByTestId("take-over-strip")).toHaveTextContent("A new Claude Code session took over from this one at");
    view.unmount();
    open(snapshotWith(joinShop, [OVER]), "s-4");
    expect(await screen.findByTestId("take-over-strip")).toHaveTextContent("Took over from the Codex session s-2 at");
    await user.hover(screen.getAllByTestId("worker").find((x) => x.dataset.session === "s-4") ?? document.body);
    expect(await screen.findByTestId("worker-take-over")).toHaveTextContent("Took over from the Codex session s-2");
  });
});
