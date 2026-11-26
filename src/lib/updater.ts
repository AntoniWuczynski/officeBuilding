import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

/** How much of an update has arrived. `total` is null when the server did not say how big it is. */
export interface DownloadProgress {
  readonly received: number;
  readonly total: number | null;
}

/** A newer release found on GitHub. */
export interface AvailableUpdate {
  readonly version: string;
  /** The release notes, or null when the release has none. */
  readonly notes: string | null;
  /** Download and install it. It does not relaunch the app. */
  readonly install: (onProgress: (progress: DownloadProgress) => void) => Promise<void>;
}

/** Ask the release feed for a newer version: null when the installed one is the latest. Rejects when the feed cannot be read. */
export async function checkForUpdate(): Promise<AvailableUpdate | null> {
  const update = await check();
  if (update === null) return null;
  return {
    version: update.version,
    notes: update.body === undefined || update.body.trim() === "" ? null : update.body,
    install: async (onProgress) => {
      let received = 0;
      let total: number | null = null;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? null;
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
        }
        onProgress({ received, total });
      });
    },
  };
}

/** Restart the app on the new version. Only ever called from a click. */
export function relaunchApp(): Promise<void> {
  return relaunch();
}
