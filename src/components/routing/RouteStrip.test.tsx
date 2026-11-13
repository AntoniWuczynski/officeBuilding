import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockRouting, makeDeskRoute } from "../../api/mockRouting";
import type { DeskRoute, ExhaustedScope } from "../../types";
import { RouteStrip } from "./RouteStrip";
import { RoutingContext } from "./RoutingContext";

const SCOPE: ExhaustedScope = { bucket: { kind: "tier", tier: "opus" }, window: null, observedAt: 1700000000, resetAt: 1, checking: false, retryArmed: false };

function exhausted(over: Partial<DeskRoute> = {}): DeskRoute {
  const base = makeDeskRoute({ requests: 3, ...over });
  const [sub, api] = base.status.entries;
  if (sub === undefined || api === undefined) throw new Error("two entries");
  return { ...base, status: { ...base.status, entries: [{ ...sub, exhausted: [SCOPE] }, api] } };
}

function show(route: DeskRoute | null): MockRouting {
  const api = new MockRouting(() => Promise.reject(new Error("no settings")));
  api.setRoute("s-1", route);
  render(
    <RoutingContext value={api}>
      <RouteStrip sessionId="s-1" />
    </RoutingContext>,
  );
  return api;
}

async function open(): Promise<void> {
  await userEvent.click(await screen.findByTestId("route-details-toggle"));
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
});
afterEach(() => {
  vi.useRealTimers();
});

describe("RouteStrip", () => {
  it("renders nothing without a provider", () => {
    render(<RouteStrip sessionId="s-1" />);
    expect(screen.queryByTestId("route-strip")).not.toBeInTheDocument();
  });

  it("does not start a read while the last one is unanswered", async () => {
    const api = new MockRouting(() => Promise.reject(new Error("no settings")));
    const read = vi.spyOn(api, "deskRoute").mockReturnValue(new Promise(() => {}));
    render(
      <RoutingContext value={api}>
        <RouteStrip sessionId="s-1" />
      </RoutingContext>,
    );
    await act(async () => {
      vi.advanceTimersByTime(10_000);
    });
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("renders nothing for a desk with no route", async () => {
    show(null);
    await act(async () => {});
    expect(screen.queryByTestId("route-strip")).not.toBeInTheDocument();
  });

  it("shows the headline of a healthy route", async () => {
    show(makeDeskRoute());
    expect(await screen.findByTestId("route-strip")).toHaveTextContent("Route: Claude login");
    expect(screen.queryByTestId("route-details")).not.toBeInTheDocument();
  });

  it("shows the move, the reasoning, a due check and the quit warning in the details", async () => {
    const base = exhausted({ reasoning: "vendor-may-drop" });
    show({ ...base, status: { ...base.status, active: "api", lastSwitch: { from: "sub", to: "api", reason: "exhausted", at: 1700000100 } } });
    await open();
    const details = screen.getByTestId("route-details");
    expect(details).toHaveTextContent("because the allowance of Claude login was used up");
    expect(details).toHaveTextContent("Anthropic leaves out whatever the new account or model cannot read");
    expect(details).toHaveTextContent("Its reset time has passed. One check is allowed");
    expect(details).toHaveTextContent("If Office Building quits while the desk is running");
    expect(screen.queryByText(/No request from this desk/)).not.toBeInTheDocument();
  });

  it("says hidden reasoning is removed", async () => {
    show(makeDeskRoute({ reasoning: "removed", requests: 1 }));
    await open();
    expect(screen.getByTestId("route-details")).toHaveTextContent("Hidden reasoning is removed from every request it sends through OpenRouter");
  });

  it("says a blocked route is blocked, and a silent one has had no request", async () => {
    const base = makeDeskRoute();
    show({ ...base, status: { ...base.status, blockedSince: 1700000000 }, recordsProblem: "the records file is unreadable" });
    expect(await screen.findByTestId("route-strip")).toHaveTextContent("Route: Blocked");
    await open();
    expect(screen.getByRole("status")).toHaveTextContent("nothing is sent until you retry one or switch");
    expect(screen.getByText("No request from this desk has reached the building yet.")).toBeInTheDocument();
    expect(screen.getByText("the records file is unreadable")).toBeInTheDocument();
  });

  it("retries a used-up connection and shows the route it resolves to", async () => {
    show(exhausted());
    await open();
    await userEvent.click(screen.getByTestId("route-retry-sub"));
    expect(await screen.findByText(/A retry is armed/)).toBeInTheDocument();
    expect(screen.queryByTestId("route-retry-sub")).not.toBeInTheDocument();
  });

  it("switches to another connection", async () => {
    show(exhausted());
    await open();
    await userEvent.click(screen.getByTestId("route-switch-api"));
    expect(await screen.findByTestId("route-strip")).toHaveTextContent("Route: Anthropic key");
    expect(screen.getByTestId("route-details")).toHaveTextContent("Switched from Claude login to Anthropic key");
  });

  it("shows the sentence of a switch that fails", async () => {
    const api = show(exhausted());
    vi.spyOn(api, "switchRoute").mockRejectedValue(new Error("Anthropic key cannot be switched to, because no API key is saved for it"));
    await open();
    await userEvent.click(screen.getByTestId("route-switch-api"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not do that: Anthropic key cannot be switched to, because no API key is saved for it.");
  });

  it("reads the route again every 3 seconds and keeps the last one when a read fails", async () => {
    const api = show(makeDeskRoute());
    const read = vi.spyOn(api, "deskRoute");
    await screen.findByTestId("route-strip");
    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    expect(read).toHaveBeenCalledTimes(1);
    read.mockRejectedValueOnce(new Error("boom"));
    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not read the route: boom.");
    expect(screen.getByTestId("route-strip")).toHaveTextContent("Route: Claude login");
  });
});
