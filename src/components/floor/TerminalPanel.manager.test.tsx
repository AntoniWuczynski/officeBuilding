import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { TerminalApiContext, TerminalPanel } from "./TerminalPanel";
import type { TerminalApi } from "./TerminalPanel";
import { makeSeedSnapshot } from "../../api/mockData";
import type { ManagerFloor, Session } from "../../types";

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options = {};
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
  },
}));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit(): void {}
  },
}));

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
});
