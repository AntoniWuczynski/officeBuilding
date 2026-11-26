import { beforeEach, describe, expect, it, vi } from "vitest";
import { checkForUpdate, relaunchApp, type DownloadProgress } from "./updater";

const plugins = vi.hoisted(() => ({ check: vi.fn(), relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: plugins.check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: plugins.relaunch }));

type Event = { event: "Started"; data: { contentLength?: number } } | { event: "Progress"; data: { chunkLength: number } } | { event: "Finished" };

beforeEach(() => {
  plugins.check.mockReset();
  plugins.relaunch.mockReset();
});

describe("checkForUpdate", () => {
  it("is null when the plugin finds nothing newer", async () => {
    plugins.check.mockResolvedValue(null);
    expect(await checkForUpdate()).toBeNull();
  });

  it("reports the version and its notes, with blank notes as null", async () => {
    plugins.check.mockResolvedValueOnce({ version: "0.3.0", body: "- one", downloadAndInstall: vi.fn() });
    expect(await checkForUpdate()).toMatchObject({ version: "0.3.0", notes: "- one" });
    plugins.check.mockResolvedValueOnce({ version: "0.3.0", body: "  ", downloadAndInstall: vi.fn() });
    expect(await checkForUpdate()).toMatchObject({ notes: null });
    plugins.check.mockResolvedValueOnce({ version: "0.3.0", downloadAndInstall: vi.fn() });
    expect(await checkForUpdate()).toMatchObject({ notes: null });
  });

  it("turns the download events into running progress", async () => {
    const events: Event[] = [{ event: "Started", data: { contentLength: 100 } }, { event: "Progress", data: { chunkLength: 30 } }, { event: "Progress", data: { chunkLength: 20 } }, { event: "Finished" }];
    plugins.check.mockResolvedValue({
      version: "0.3.0",
      downloadAndInstall: (onEvent: (e: Event) => void) => {
        events.forEach(onEvent);
        return Promise.resolve();
      },
    });
    const seen: DownloadProgress[] = [];
    await (await checkForUpdate())?.install((p) => seen.push(p));
    expect(seen).toEqual([
      { received: 0, total: 100 },
      { received: 30, total: 100 },
      { received: 50, total: 100 },
      { received: 50, total: 100 },
    ]);
  });

  it("passes on a failed check", async () => {
    plugins.check.mockRejectedValue(new Error("offline"));
    await expect(checkForUpdate()).rejects.toThrow("offline");
  });
});

describe("relaunchApp", () => {
  it("calls the process plugin once", async () => {
    plugins.relaunch.mockResolvedValue(undefined);
    await relaunchApp();
    expect(plugins.relaunch).toHaveBeenCalledTimes(1);
  });
});
