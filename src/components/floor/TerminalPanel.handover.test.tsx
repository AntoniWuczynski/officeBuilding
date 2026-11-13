import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { makeSeedSnapshot } from "../../api/mockData";
import { makeHandoverView } from "../../api/mockHandover";
import { floor } from "./floorHarness";
import { TerminalApiContext, TerminalPanel } from "./TerminalPanel";
import type { TerminalApi } from "./TerminalPanel";
import type { DeskHandoverState } from "../../types";

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
