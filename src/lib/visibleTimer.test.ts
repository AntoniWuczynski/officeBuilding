import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { everyWhileVisible } from "./visibleTimer";

let state: DocumentVisibilityState = "visible";
const setVisibility = (next: DocumentVisibilityState): void => {
  state = next;
  document.dispatchEvent(new Event("visibilitychange"));
};

describe("everyWhileVisible", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    state = "visible";
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("ticks on its interval while the window shows", () => {
    const tick = vi.fn();
    const stop = everyWhileVisible(tick, 1000);
    vi.advanceTimersByTime(3000);
    expect(tick).toHaveBeenCalledTimes(3);
    stop();
  });

  it("skips ticks while hidden and catches up once on coming back", () => {
    const tick = vi.fn();
    const stop = everyWhileVisible(tick, 1000);
    setVisibility("hidden");
    vi.advanceTimersByTime(5000);
    expect(tick).not.toHaveBeenCalled();
    setVisibility("visible");
    expect(tick).toHaveBeenCalledTimes(1);
    stop();
  });

  it("stops ticking and listening once stopped", () => {
    const tick = vi.fn();
    everyWhileVisible(tick, 1000)();
    vi.advanceTimersByTime(3000);
    setVisibility("hidden");
    setVisibility("visible");
    expect(tick).not.toHaveBeenCalled();
  });
});
