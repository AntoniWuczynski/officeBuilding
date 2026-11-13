import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => undefined) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => undefined) }) }));

const { TauriHandover } = await import("./tauriHandover");

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
});

describe("TauriHandover", () => {
  it("sends the session id and nothing else", async () => {
    await new TauriHandover().handOverDesk("s-1");
    expect(invoke).toHaveBeenCalledWith("hand_over_desk", { sessionId: "s-1" });
  });
  it("passes a rejection on", async () => {
    invoke.mockRejectedValueOnce("Codex was not found");
    await expect(new TauriHandover().handOverDesk("s-1")).rejects.toThrow(/Codex was not found/);
  });
});
