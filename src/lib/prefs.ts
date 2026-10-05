/** A per-viewer preference from the webview's storage, or null when unset or storage is unavailable. */
export function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** Keep a preference, or forget it with null. Without storage it holds only for this run. */
export function writePref(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // Storage unavailable: the caller's state still holds for this run.
  }
}
