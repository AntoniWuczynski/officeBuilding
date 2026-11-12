import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeSeedSnapshot } from "../../api/mockData";
import type { Project, QaFloor } from "../../types";
import { QaContext } from "./QaContext";
import type { QaActions } from "./QaContext";
import { QaRow } from "./QaRow";

const NOW = new Date("2026-09-28T15:45:00Z");
const snapshot = makeSeedSnapshot();

function found<T>(item: T | undefined): T {
  if (item === undefined) throw new Error("missing seed entry");
  return item;
}

const shop: Project = found(snapshot.projects.find((p) => p.id === "p-shop"));
const shopSessions = snapshot.sessions.filter((s) => s.projectId === "p-shop");
const shopQa: QaFloor = found(snapshot.qa.find((q) => q.projectId === "p-shop"));
const off: QaFloor = { projectId: "p-shop", enabled: false, status: { kind: "off" }, run: null, reviews: [], stamp: null };

function actionsFor(over: Partial<QaActions> = {}): QaActions {
  return { setFloor: vi.fn(() => Promise.resolve()), recheck: vi.fn(), openReview: vi.fn(), openSettings: vi.fn(), ...over };
}

function show(qa: QaFloor | undefined, actions: QaActions | null, onOpenSession = vi.fn()): void {
  render(
    <QaContext value={actions}>
      <dl>
        <QaRow project={shop} qa={qa} sessions={shopSessions} now={NOW} onOpenSession={onOpenSession} />
      </dl>
    </QaContext>,
  );
}

describe("QaRow", () => {
  it("renders nothing without a provider or without a QA entry", () => {
    show(shopQa, null);
    expect(screen.queryByText("Quality assurance")).not.toBeInTheDocument();
  });

  it("renders nothing for a floor with no QA entry", () => {
    show(undefined, actionsFor());
    expect(screen.queryByText("Quality assurance")).not.toBeInTheDocument();
  });

  it("asks before switching on, and calls nothing until Turn on", async () => {
    const actions = actionsFor();
    show(off, actions);
    await userEvent.click(screen.getByRole("switch"));
    expect(screen.getByTestId("qa-confirm")).toHaveTextContent("posted on GitHub from your account");
    expect(actions.setFloor).not.toHaveBeenCalled();
    await userEvent.click(screen.getByTestId("qa-confirm-yes"));
    expect(actions.setFloor).toHaveBeenCalledWith("p-shop", true);
    expect(screen.queryByTestId("qa-confirm")).not.toBeInTheDocument();
  });

  it("leaves QA off on Cancel, or when the switch is flipped back", async () => {
    const actions = actionsFor();
    show(off, actions);
    await userEvent.click(screen.getByRole("switch"));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("qa-confirm")).not.toBeInTheDocument();
    expect(screen.getByRole("switch")).not.toBeChecked();
    await userEvent.click(screen.getByRole("switch"));
    await userEvent.click(screen.getByRole("switch"));
    expect(screen.queryByTestId("qa-confirm")).not.toBeInTheDocument();
    expect(actions.setFloor).not.toHaveBeenCalled();
  });

  it("switches off at once", async () => {
    const actions = actionsFor();
    show(shopQa, actions);
    await userEvent.click(screen.getByRole("switch"));
    expect(actions.setFloor).toHaveBeenCalledWith("p-shop", false);
  });

  it("shows why a switch failed", async () => {
    show(off, actionsFor({ setFloor: () => Promise.reject(new Error("store is read-only")) }));
    await userEvent.click(screen.getByRole("switch"));
    await userEvent.click(screen.getByTestId("qa-confirm-yes"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not switch QA: store is read-only.");
  });

  it("says what the desk is doing and opens the reviewer's desk when it is on the floor", async () => {
    const onOpenSession = vi.fn();
    show(shopQa, actionsFor(), onOpenSession);
    expect(screen.getByTestId("qa-status")).toHaveTextContent("Reviewing #14 “Cache the price list”.");
    await userEvent.click(screen.getByTestId("qa-open-desk"));
    expect(onOpenSession).toHaveBeenCalledWith(shopSessions.find((s) => s.id === "s-1"));
  });

  it("offers no desk to open when the run's session is not on the floor", () => {
    show({ ...shopQa, run: shopQa.run === null ? null : { ...shopQa.run, sessionId: "s-gone" } }, actionsFor());
    expect(screen.queryByTestId("qa-open-desk")).not.toBeInTheDocument();
  });

  it("hides the status and checks when QA is off", () => {
    show(off, actionsFor());
    expect(screen.queryByTestId("qa-status")).not.toBeInTheDocument();
    expect(screen.queryByTestId("qa-recheck")).not.toBeInTheDocument();
  });

  it("checks now and opens Settings", async () => {
    const actions = actionsFor();
    show(shopQa, actions);
    await userEvent.click(screen.getByTestId("qa-recheck"));
    expect(actions.recheck).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(actions.openSettings).toHaveBeenCalledOnce();
  });

  it("lists the last reviews with their outcome and age, each opening its review", async () => {
    const actions = actionsFor();
    show({ ...shopQa, enabled: false }, actions);
    const first = found(screen.getAllByTestId("qa-review")[0]);
    expect(first).toHaveAttribute("aria-label", expect.stringMatching(/^Open the review of pull request 12, /));
    expect(found(first.parentElement)).toHaveTextContent("2 findings, ");
    await userEvent.click(first);
    expect(actions.openReview).toHaveBeenCalledWith("p-shop", shopQa.reviews[0]);
  });
});
