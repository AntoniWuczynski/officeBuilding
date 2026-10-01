import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { TerminalApiContext, TerminalPanel } from "./TerminalPanel";
import type { TerminalApi } from "./TerminalPanel";
import { makeSeedSnapshot } from "../../api/mockData";
import type { Session } from "../../types";

const xterm = vi.hoisted(() => {
  class FakeTerminal {
    cols = 80;
    rows = 24;
    readonly options: { disableStdin?: boolean } = {};
    loadAddon(): void {}
    open(): void {}
    write(): void {}
    onData(): { dispose(): void } {
      return { dispose: () => {} };
    }
    onResize(): { dispose(): void } {
      return { dispose: () => {} };
    }
    focus(): void {}
    dispose(): void {}
  }
  class FakeFitAddon {
    fit(): void {}
  }
  return { FakeTerminal, FakeFitAddon };
});
vi.mock("@xterm/xterm", () => ({ Terminal: xterm.FakeTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: xterm.FakeFitAddon }));

function fullSession(): Session {
  const s = makeSeedSnapshot().sessions.find((x) => x.control === "full");
  if (s === undefined) throw new Error("seed has no full session");
  return s;
}

describe("TerminalPanel (verifier)", () => {
  // FAILS: after `/clear` in an in-app terminal, discovery reports the old session id
  // read-only and the backend rebinds the (still running) terminal to the new id.
  // The open panel, keyed by the old id, drops the live terminal and tells the user
  // the session "was started outside the app", which is false.
  it("verifier_after_clear_the_open_panel_does_not_claim_the_session_started_outside_the_app", () => {
    const stop = vi.fn();
    const api: TerminalApi = {
      subscribeTerminal: () => stop,
      writeTerminal: () => Promise.resolve(),
      resizeTerminal: () => Promise.resolve(),
    };
    const session = fullSession();
    const { rerender } = render(
      <TerminalApiContext value={api}>
        <TerminalPanel session={session} onClose={() => {}} />
      </TerminalApiContext>,
    );
    expect(screen.getByTestId("terminal-screen")).toBeInTheDocument();
    rerender(
      <TerminalApiContext value={api}>
        <TerminalPanel session={{ ...session, control: "read-only" }} onClose={() => {}} />
      </TerminalApiContext>,
    );
    expect(screen.queryByText(/started outside the app/)).not.toBeInTheDocument();
  });
});
