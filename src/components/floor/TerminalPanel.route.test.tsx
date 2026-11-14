import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { TerminalApiContext, TerminalPanel } from "./TerminalPanel";
import type { TerminalApi } from "./TerminalPanel";
import { MockRouting, makeDeskRoute } from "../../api/mockRouting";
import { makeSeedSnapshot } from "../../api/mockData";
import { RoutingContext } from "../routing/RoutingContext";
import type { Session } from "../../types";


const api: TerminalApi = {
  subscribeTerminal: () => () => {},
  writeTerminal: () => Promise.resolve(),
  resizeTerminal: () => Promise.resolve(),
  subscribeFileDrops: () => () => {},
};

function show(id: string): void {
  const session: Session | undefined = makeSeedSnapshot().sessions.find((s) => s.id === id);
  if (session === undefined) throw new Error(`seed is missing ${id}`);
  const routing = new MockRouting(() => Promise.reject(new Error("no settings")));
  routing.setRoute(id, makeDeskRoute());
  render(
    <RoutingContext value={routing}>
      <TerminalApiContext value={api}>
        <TerminalPanel session={session} manager={undefined} onClose={() => {}} />
      </TerminalApiContext>
    </RoutingContext>,
  );
}

describe("TerminalPanel route strip", () => {
  it("shows the strip for a Claude Code desk in the app's terminal that has a route", async () => {
    show("s-1");
    expect(await screen.findByTestId("route-strip")).toBeInTheDocument();
  });

  it("does not render it for a Codex desk", async () => {
    show("s-2");
    await screen.findByTestId("terminal-close");
    expect(screen.queryByTestId("route-strip")).not.toBeInTheDocument();
  });
});
