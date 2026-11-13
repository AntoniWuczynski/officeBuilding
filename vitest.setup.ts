import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// jsdom has no layout engine and no ResizeObserver; scenes render at scale 1.
class NoopResizeObserver implements ResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver = NoopResizeObserver;

// jsdom lacks AnimationEvent, which makes React listen for the prefixed
// `webkitAnimationEnd` instead of `animationend`. Provide it so tests can end walks.
class AnimationEventShim extends Event implements AnimationEvent {
  readonly animationName: string;
  readonly elapsedTime: number;
  readonly pseudoElement: string;
  constructor(type: string, init: AnimationEventInit = {}) {
    super(type, init);
    this.animationName = init.animationName ?? "";
    this.elapsedTime = init.elapsedTime ?? 0;
    this.pseudoElement = init.pseudoElement ?? "";
  }
}
globalThis.AnimationEvent = AnimationEventShim;

// jsdom has no pointer capture; the floor's drag-to-pan takes it.
Element.prototype.setPointerCapture = () => {};

// xterm needs a canvas jsdom lacks; every test that does not bring its own fake gets this stub.
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options = {};
    loadAddon(): void {}
    open(): void {}
    write(_data: string, done?: () => void): void {
      done?.();
    }
    resize(): void {}
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
