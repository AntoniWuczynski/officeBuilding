/** A per-viewer preference from the webview's storage, or null when unset or storage is unavailable. */
export function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** A preference kept as JSON, or null when unset, unavailable or not JSON. */
export function readJsonPref(key: string): unknown {
  try {
    return JSON.parse(readPref(key) ?? "null");
  } catch {
    return null;
  }
}

/** The entries of a stored object whose values pass `is`; anything but an object has none. */
export function entriesOf<T>(v: unknown, is: (x: unknown) => x is T): Record<string, T> {
  return typeof v === "object" && v !== null ? Object.fromEntries(Object.entries(v).filter((e): e is [string, T] => is(e[1]))) : {};
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
