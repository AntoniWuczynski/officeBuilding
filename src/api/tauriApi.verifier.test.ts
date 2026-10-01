import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriApi } from "./tauriApi";

const invoke = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();
const unlistenOutput = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string) =>
    event === "office://terminal-exit" ? Promise.reject(new Error("listen failed")) : Promise.resolve(unlistenOutput),
}));

async function flush(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

describe("TauriApi.subscribeTerminal (verifier)", () => {
  beforeEach(() => {
    invoke.mockReset();
    unlistenOutput.mockReset();
  });

  // FAILS: when one of the two `listen` calls rejects, Promise.all drops the other
  // one's unlisten function, so the output listener stays registered for the life
  // of the webview even after the panel stops watching.
  it("verifier_a_failed_listen_does_not_leak_the_other_listener", async () => {
    const errors: string[] = [];
    const stop = new TauriApi().subscribeTerminal("s-1", { output: () => {}, exit: () => {}, error: (m) => errors.push(m) });
    await flush();
    expect(errors).toEqual(["listen failed"]);
    stop();
    expect(unlistenOutput).toHaveBeenCalledTimes(1);
  });
});
