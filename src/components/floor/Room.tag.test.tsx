import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { makeSeedSnapshot } from "../../api/mockData";
import { floor } from "./floorHarness";

describe("a note walking across a floor", () => {
  it("carries the same 'sender to recipient' tag a boat or parrot carries on the island map, and only for a message new since open", () => {
    const seed = makeSeedSnapshot();
    const old = { id: "m-old", fromSessionId: "s-1", toSessionId: "s-2", text: "earlier", sentAt: "2026-09-28T15:00:00Z" };
    const first = { ...seed, messages: [old] };
    const { rerender } = render(floor(first, "p-shop"));
    expect(screen.queryByTestId("walk")).not.toBeInTheDocument();
    rerender(floor({ ...first, messages: [old, { ...old, id: "m-new", text: "rebase onto mine" }] }, "p-shop"));
    const walk = screen.getByTestId("walk");
    expect(walk).toHaveTextContent("audit checkout to orders ingest…");
  });
});
