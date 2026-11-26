import { useSyncExternalStore } from "react";
import { message } from "../lib/errors";
import { checkForUpdate, relaunchApp, type AvailableUpdate, type DownloadProgress } from "../lib/updater";

export type Phase =
  | { readonly kind: "idle" }
  | { readonly kind: "checking" }
  | { readonly kind: "latest" }
  | { readonly kind: "available"; readonly update: AvailableUpdate }
  | { readonly kind: "downloading"; readonly update: AvailableUpdate; readonly progress: DownloadProgress | null }
  | { readonly kind: "installed"; readonly version: string }
  | { readonly kind: "failed"; readonly text: string; readonly retry: "check" | "install" | "relaunch"; readonly update: AvailableUpdate | null };

/**
 * The update's state lives here, not in the Settings panel: the panel unmounts when it is closed, and a
 * download or an installed update must still be there when it is opened again.
 */
let phase: Phase = { kind: "idle" };
const listeners = new Set<() => void>();

function set(next: Phase): void {
  phase = next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useUpdatePhase(): Phase {
  return useSyncExternalStore(subscribe, () => phase);
}

/** Back to idle, for tests: the store is shared by everything in one page. */
export function resetUpdate(): void {
  set({ kind: "idle" });
}

export function runCheck(): void {
  set({ kind: "checking" });
  checkForUpdate().then(
    (update) => set(update === null ? { kind: "latest" } : { kind: "available", update }),
    (err: unknown) => set({ kind: "failed", text: message(err), retry: "check", update: null }),
  );
}

export function runInstall(update: AvailableUpdate): void {
  set({ kind: "downloading", update, progress: null });
  update
    .install((progress: DownloadProgress) => set({ kind: "downloading", update, progress }))
    .then(
      () => set({ kind: "installed", version: update.version }),
      (err: unknown) => set({ kind: "failed", text: message(err), retry: "install", update }),
    );
}

export function runRelaunch(): void {
  relaunchApp().catch((err: unknown) => set({ kind: "failed", text: message(err), retry: "relaunch", update: null }));
}
