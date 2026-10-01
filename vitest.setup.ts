import "@testing-library/jest-dom/vitest";

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
