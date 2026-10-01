import { beforeEach, describe, expect, it, vi } from "vitest";
import { makeSeedSnapshot } from "./mockData";
import { makeHireOptions } from "./mockHireOptions";
import { hireOptionsSchema, parseWire, snapshotSchema } from "./wire";

const invoke = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();
const listeners = new Map<string, (event: { payload: unknown }) => void>();
const unlisten = vi.fn();

function push(event: string, payload: unknown): void {
  listeners.get(event)?.({ payload });
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(event, handler);
    return Promise.resolve(unlisten);
  },
}));

const { TauriApi } = await import("./tauriApi");

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

describe("wire", () => {
  it("accepts the shapes the app uses", () => {
    expect(parseWire(snapshotSchema, "snapshot", makeSeedSnapshot())).toEqual(makeSeedSnapshot());
    expect(parseWire(hireOptionsSchema, "options", makeHireOptions())).toHaveLength(2);
  });
  it("names where a payload went wrong", () => {
    const bad = { ...makeSeedSnapshot(), sessions: [{ id: 1 }] };
    expect(() => parseWire(snapshotSchema, "snapshot", bad)).toThrow(/unexpected snapshot at sessions\.0\.id/);
    expect(() => parseWire(snapshotSchema, "snapshot", null)).toThrow(/at the top level/);
  });
});

describe("TauriApi", () => {
  beforeEach(() => {
    invoke.mockReset();
    unlisten.mockReset();
    listeners.clear();
  });

  it("calls each command with camelCase arguments", async () => {
    const api = new TauriApi();
    const session = makeSeedSnapshot().sessions[0];
    const floor = makeSeedSnapshot().projects[0];
    invoke
      .mockResolvedValueOnce(makeHireOptions())
      .mockResolvedValueOnce(session)
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(floor)
      .mockResolvedValueOnce("/Users/me/code/app");
    await api.hireOptions();
    const request = { tool: "codex", title: "x", model: "gpt-5.5", effort: "high" } as const;
    await expect(api.spawnSession("p-1", request)).resolves.toEqual(session);
    await api.answerQuestion("q-1", { kind: "option", optionId: "o-1" });
    await api.tickTodo("t-1");
    await api.sendAgentMessage("s-1", "s-2", "hi");
    await api.focusSession("s-1");
    await expect(api.addFloor("~/code/app")).resolves.toEqual(floor);
    await expect(api.pickFolder()).resolves.toBe("/Users/me/code/app");
    expect(api.canPickFolder).toBe(true);
    expect(invoke.mock.calls).toEqual([
      ["hire_options", undefined],
      ["spawn_session", { projectId: "p-1", request }],
      ["answer_question", { questionId: "q-1", answer: { kind: "option", optionId: "o-1" } }],
      ["tick_todo", { todoId: "t-1" }],
      ["send_agent_message", { fromSessionId: "s-1", toSessionId: "s-2", text: "hi" }],
      ["focus_session", { sessionId: "s-1" }],
      ["add_floor", { path: "~/code/app" }],
      ["pick_folder", undefined],
    ]);
  });

  it("turns rejected commands into errors carrying the backend's message", async () => {
    invoke.mockRejectedValueOnce("its process has exited").mockRejectedValueOnce(new Error("boom"));
    const api = new TauriApi();
    await expect(api.focusSession("s-1")).rejects.toThrow("its process has exited");
    await expect(api.tickTodo("t")).rejects.toThrow("boom");
  });

  it("rejects a snapshot of the wrong shape", async () => {
    invoke.mockResolvedValueOnce({ projects: "nope" });
    await expect(new TauriApi().getSnapshot()).rejects.toThrow(/unexpected snapshot/);
  });

  it("subscribe delivers the first snapshot, then valid pushes, and unlistens", async () => {
    const snap = makeSeedSnapshot();
    invoke.mockResolvedValue(snap);
    const listener = vi.fn();
    const off = new TauriApi().subscribe(listener);
    await flush();
    expect(listener).toHaveBeenCalledWith(snap);
    push("office://snapshot", { ...snap, projects: [] });
    push("office://snapshot", "garbage");
    expect(listener).toHaveBeenCalledTimes(2);
    off();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("subscribe stopped before the listener attaches still cleans up and stays silent", async () => {
    invoke.mockResolvedValue(makeSeedSnapshot());
    const listener = vi.fn();
    const off = new TauriApi().subscribe(listener);
    off();
    await flush();
    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(listener).not.toHaveBeenCalled();
  });

  describe("terminals", () => {
    const OUTPUT = "office://terminal-output";
    const EXIT = "office://terminal-exit";
    const backlog = { terminalId: "t-1", seq: 2, data: "old", running: true, exitCode: null, cols: 100, rows: 24 };

    function watch(): { calls: string[]; stop: () => void } {
      const calls: string[] = [];
      const stop = new TauriApi().subscribeTerminal("s-1", {
        output: (d) => calls.push(`out:${d}`),
        exit: (c) => calls.push(`exit:${String(c)}`),
        error: (m) => calls.push(`error:${m}`),
      });
      return { calls, stop };
    }

    it("replays the backlog, then only newer chunks of its own terminal, and exits once", async () => {
      let answer: (value: unknown) => void = () => {};
      invoke.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)));
      const { calls, stop } = watch();
      await flush();
      expect(invoke).toHaveBeenCalledWith("terminal_backlog", { sessionId: "s-1" });
      // Arrives while the backlog is in flight: seq 2 is already in it.
      push(OUTPUT, { terminalId: "t-1", seq: 2, data: "old" });
      push(OUTPUT, { terminalId: "t-1", seq: 3, data: "new" });
      push(OUTPUT, { terminalId: "t-2", seq: 9, data: "other" });
      push(OUTPUT, { terminalId: "t-1" });
      push(EXIT, "garbage");
      answer(backlog);
      await flush();
      push(OUTPUT, { terminalId: "t-1", seq: 4, data: "live" });
      push(OUTPUT, { terminalId: "t-1", seq: 4, data: "live" });
      push(EXIT, { terminalId: "t-2", exitCode: 1 });
      push(EXIT, { terminalId: "t-1", exitCode: 0 });
      push(EXIT, { terminalId: "t-1", exitCode: 0 });
      expect(calls).toEqual(["out:old", "out:new", "out:live", "exit:0"]);
      stop();
      expect(unlisten).toHaveBeenCalledTimes(2);
      push(OUTPUT, { terminalId: "t-1", seq: 5, data: "late" });
      expect(calls).toHaveLength(4);
    });

    it("a finished terminal replays nothing new and reports its exit", async () => {
      invoke.mockResolvedValueOnce({ ...backlog, data: "", running: false, exitCode: 3 });
      const { calls } = watch();
      await flush();
      expect(calls).toEqual(["exit:3"]);
    });

    it("reports a terminal it cannot reach or a backlog of the wrong shape", async () => {
      invoke.mockRejectedValueOnce("s-1 was not started in the app, so it has no terminal here");
      const first = watch();
      await flush();
      expect(first.calls).toEqual(["error:s-1 was not started in the app, so it has no terminal here"]);
      invoke.mockResolvedValueOnce({ terminalId: 1 });
      const second = watch();
      await flush();
      expect(second.calls[0]).toMatch(/^error:the backend sent an unexpected terminal backlog/);
    });

    it("stopping early leaves nothing listening and delivers nothing", async () => {
      invoke.mockResolvedValue(backlog);
      const before = watch();
      before.stop();
      await flush();
      expect(unlisten).toHaveBeenCalledTimes(2);
      expect(invoke).not.toHaveBeenCalled();

      let answer: (value: unknown) => void = () => {};
      invoke.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)));
      const during = watch();
      await flush();
      during.stop();
      push(OUTPUT, { terminalId: "t-1", seq: 3, data: "new" });
      answer(backlog);
      await flush();
      invoke.mockRejectedValueOnce("gone");
      const failing = watch();
      await flush();
      failing.stop();
      expect([...before.calls, ...during.calls, ...failing.calls]).toEqual(["error:gone"]);
    });

    it("sends keystrokes and sizes to the session's terminal", async () => {
      invoke.mockResolvedValue(null);
      const api = new TauriApi();
      await api.writeTerminal("s-1", "ls\r");
      await api.resizeTerminal("s-1", 120, 30);
      expect(invoke.mock.calls).toEqual([
        ["terminal_write", { sessionId: "s-1", data: "ls\r" }],
        ["terminal_resize", { sessionId: "s-1", cols: 120, rows: 30 }],
      ]);
    });
  });

  describe("hire failures", () => {
    it("delivers valid reports until unsubscribed", async () => {
      const reports = vi.fn();
      const off = new TauriApi().subscribeHireFailures(reports);
      await flush();
      const failure = { title: "fix it", exitCode: 127, lastLines: ["zsh: command not found: claude"] };
      push("office://hire-failed", failure);
      push("office://hire-failed", { title: 1 });
      expect(reports.mock.calls).toEqual([[failure]]);
      off();
      expect(unlisten).toHaveBeenCalledTimes(1);
    });

    it("stopping before the listener attaches leaves nothing listening", async () => {
      const reports = vi.fn();
      new TauriApi().subscribeHireFailures(reports)();
      await flush();
      expect(unlisten).toHaveBeenCalledTimes(1);
      push("office://hire-failed", { title: "late", exitCode: null, lastLines: [] });
      expect(reports).not.toHaveBeenCalled();
    });
  });
});
