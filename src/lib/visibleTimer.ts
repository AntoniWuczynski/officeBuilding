const showing = (): boolean => document.visibilityState !== "hidden";

/** `setInterval` that skips ticks while the window is hidden or minimised, and ticks once on coming back. Returns the stop function. */
export function everyWhileVisible(tick: () => void, ms: number): () => void {
  const timer = setInterval(() => {
    if (showing()) tick();
  }, ms);
  const onChange = (): void => {
    if (showing()) tick();
  };
  document.addEventListener("visibilitychange", onChange);
  return () => {
    clearInterval(timer);
    document.removeEventListener("visibilitychange", onChange);
  };
}
