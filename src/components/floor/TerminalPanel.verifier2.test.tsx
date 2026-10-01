import { describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import { TerminalApiContext, TerminalPanel } from "./TerminalPanel";
import type { TerminalApi } from "./TerminalPanel";
import { makeSeedSnapshot } from "../../api/mockData";
import type { Session } from "../../types";
import type { FileDrop } from "../../api/DiscoveryApi";

const xterm = vi.hoisted(() => {
  class FakeTerminal {
    static last: FakeTerminal | null = null;
    cols = 80;
    rows = 24;
    readonly options: { disableStdin?: boolean } = {};
    private listener: ((data: string) => void) | null = null;
    constructor() {
      FakeTerminal.last = this;
    }
    loadAddon(): void {}
    open(): void {}
    write(): void {}
    onData(listener: (data: string) => void): { dispose(): void } {
      this.listener = listener;
      return { dispose: () => {} };
    }
    onResize(): { dispose(): void } {
      return { dispose: () => {} };
    }
    focus(): void {}
    dispose(): void {}
    type(data: string): void {
      this.listener?.(data);
    }
    paste(data: string): void {
      this.listener?.(data);
    }
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

describe("TerminalPanel (verifier round 2)", () => {
  // FAILS: one refresh can briefly unbind our own session (office.rs claim_owned
  // treats a missed pid lookup as "live elsewhere"), so one keystroke fails. The
  // alert is never cleared when later keystrokes go through, so the panel keeps
  // saying "Terminal problem: … was not started in the app" on a working terminal.
  it("verifier2_a_transient_write_failure_does_not_leave_a_permanent_alert", async () => {
    const writeTerminal = vi
      .fn<TerminalApi["writeTerminal"]>()
      .mockRejectedValueOnce(new Error("s-1 was not started in the app, so it has no terminal here"))
      .mockResolvedValue(undefined);
    const api: TerminalApi = {
      subscribeTerminal: () => () => {},
      writeTerminal,
      resizeTerminal: () => Promise.resolve(),
      subscribeFileDrops: () => () => {},
    };
    render(
      <TerminalApiContext value={api}>
        <TerminalPanel session={fullSession()} onClose={() => {}} />
      </TerminalApiContext>,
    );
    const term = xterm.FakeTerminal.last;
    if (term === null) throw new Error("no terminal");
    act(() => term.type("a"));
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    act(() => term.type("b"));
    await waitFor(() => expect(writeTerminal).toHaveBeenCalledTimes(2));
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("TerminalPanel file drops", () => {
  function renderWithDrops(): { drop: (d: FileDrop) => void; writeTerminal: ReturnType<typeof vi.fn<TerminalApi["writeTerminal"]>> } {
    let listener: ((d: FileDrop) => void) | null = null;
    const writeTerminal = vi.fn<TerminalApi["writeTerminal"]>().mockResolvedValue(undefined);
    const api: TerminalApi = {
      subscribeTerminal: () => () => {},
      writeTerminal,
      resizeTerminal: () => Promise.resolve(),
      subscribeFileDrops: (l) => {
        listener = l;
        return () => {};
      },
    };
    render(
      <TerminalApiContext value={api}>
        <TerminalPanel session={fullSession()} onClose={() => {}} />
      </TerminalApiContext>,
    );
    const screenEl = screen.getByTestId("terminal-screen");
    screenEl.getBoundingClientRect = () => DOMRect.fromRect({ x: 0, y: 500, width: 800, height: 300 });
    return { drop: (d) => act(() => listener?.(d)), writeTerminal };
  }

  it("types a screenshot dropped on the terminal as its escaped path", () => {
    const { drop, writeTerminal } = renderWithDrops();
    drop({ paths: ["/Users/me/Desktop/Screenshot 1.png"], x: 400, y: 600 });
    expect(writeTerminal).toHaveBeenCalledWith(fullSession().id, "/Users/me/Desktop/Screenshot\\ 1.png");
  });

  it("ignores files dropped elsewhere in the window", () => {
    const { drop, writeTerminal } = renderWithDrops();
    drop({ paths: ["/tmp/a.png"], x: 400, y: 100 });
    expect(writeTerminal).not.toHaveBeenCalled();
  });
});
