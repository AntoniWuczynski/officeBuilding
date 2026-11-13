import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockHandover } from "../../api/mockHandover";
import type { DeskHandoverState } from "../../types";
import { HandoverContext } from "./HandoverContext";
import { HandoverStrip } from "./HandoverStrip";

const offered: DeskHandoverState = { kind: "offered", to: "codex", why: { kind: "automatic-off" }, work: { kind: "running", tasks: [{ kind: "workflow", name: "release" }] }, window: "five-hour", resetAt: null };

function show(state: DeskHandoverState, api: MockHandover | null = new MockHandover()): void {
  render(
    <HandoverContext value={api}>
      <HandoverStrip sessionId="s-1" state={state} />
    </HandoverContext>,
  );
}

describe("HandoverStrip", () => {
  it("says what an offered desk waits for, with a button", () => {
    show(offered);
    expect(screen.getByTestId("handover-strip")).toHaveTextContent("The Claude subscription is used up. The reset time is not known. Automatic handover is off, so it waits for you.");
    expect(screen.getByTestId("handover-button")).toHaveTextContent("Hand over to Codex");
  });

  it("has no button without a provider or for any other state", () => {
    show(offered, null);
    expect(screen.queryByTestId("handover-button")).not.toBeInTheDocument();
  });

  it.each<[string, DeskHandoverState, string]>([
    ["blocked", { kind: "blocked", to: "codex", why: "not-chosen", window: null, resetAt: null, otherResetAt: null }, "choose what Codex starts with in Settings, under Handover."],
    ["arrived", { kind: "arrived", from: "claude-code", at: 0, window: null, resetAt: null }, "Handed over from Claude Code at"],
  ])("shows a %s line and no button", (_name, state, text) => {
    show(state);
    expect(screen.getByTestId("handover-strip")).toHaveTextContent(text);
    expect(screen.queryByTestId("handover-button")).not.toBeInTheDocument();
  });

  it("confirms by naming what is ended, hands over once, and can be cancelled", async () => {
    const user = userEvent.setup();
    const api = new MockHandover();
    show(offered, api);
    await user.click(screen.getByTestId("handover-button"));
    expect(screen.getByTestId("handover-confirm")).toHaveTextContent("and with it the workflow “release”.");
    await user.click(screen.getByTestId("handover-confirm-no"));
    expect(screen.queryByTestId("handover-confirm")).not.toBeInTheDocument();
    await user.click(screen.getByTestId("handover-button"));
    await user.click(screen.getByTestId("handover-confirm-yes"));
    expect(api.handed).toEqual(["s-1"]);
    expect(screen.queryByTestId("handover-confirm")).not.toBeInTheDocument();
  });

  it("keeps the row and shows the reason when the handover fails", async () => {
    const user = userEvent.setup();
    const api = new MockHandover();
    api.failNext("no folder");
    show(offered, api);
    await user.click(screen.getByTestId("handover-button"));
    await user.click(screen.getByTestId("handover-confirm-yes"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not hand over: no folder.");
    expect(screen.getByTestId("handover-confirm")).toBeInTheDocument();
  });
});
