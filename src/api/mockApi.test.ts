import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "./mockApi";
import { STRUCTURED } from "./mockConversation";
import { makeSeedSnapshot } from "./mockData";
import { addContext } from "../lib/routing";
import { makeSettings } from "./mockSettings";

const NOW = new Date("2026-09-29T12:00:00Z");
const fixedNow = (): Date => NOW;

/** A deterministic random source that replays `values` in order, then repeats the last. */
function sequence(...values: number[]): () => number {
  let i = 0;
  return () => values[Math.min(i++, values.length - 1)] ?? 0;
}

describe("MockApi", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns the seed snapshot", async () => {
    const api = new MockApi();
    const snap = await api.getSnapshot();
    expect(snap.projects.map((p) => p.id)).toContain("p-shop");
    expect(snap.sessions).toHaveLength(6);
    expect(snap.questions).toHaveLength(3);
    expect(snap.messages).toEqual([]);
  });

  it("subscribe emits the current snapshot immediately and unsubscribes", () => {
    const api = new MockApi();
    const listener = vi.fn();
    const off = api.subscribe(listener);
    expect(listener).toHaveBeenCalledTimes(1);
    off();
    void api.tickTodo("t-1");
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("offers the hireable tools with per-model efforts", async () => {
    const options = await new MockApi().hireOptions();
    expect(options.map((o) => o.tool)).toEqual(["claude-code", "codex", "antigravity", "opencode", "cursor"]);
    for (const tool of options.filter((o) => o.available)) {
      expect(tool.models.some((m) => m.id === tool.defaultModel)).toBe(true);
      for (const m of tool.models) expect(m.efforts.some((e) => e.id === m.defaultEffort)).toBe(true);
    }
    const codex = options.find((o) => o.tool === "codex");
    const effortsOf = (id: string): string[] => codex?.models.find((m) => m.id === id)?.efforts.map((e) => e.id) ?? [];
    expect(effortsOf("gpt-5.5")).not.toContain("max");
    expect(effortsOf("gpt-6-astra")).toContain("ultra");
  });

  it("spawnSession adds a thinking, app-controlled session with its model and effort", async () => {
    const api = new MockApi(makeSeedSnapshot(), fixedNow);
    const listener = vi.fn();
    api.subscribe(listener);
    const session = await api.spawnSession("p-arcade", { tool: "codex", title: "  port the renderer ", model: "gpt-5.5", effort: "high", ultracode: false, view: "terminal" });
    expect(session).toMatchObject({ projectId: "p-arcade", tool: "codex", state: "thinking", control: "full", title: "port the renderer", model: "gpt-5.5", effort: "high", lastActivityAt: NOW.toISOString() });
    const snap = await api.getSnapshot();
    expect(snap.projects.find((p) => p.id === "p-arcade")?.sessionIds).toContain(session.id);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("a chat desk's backlog carries the structured capabilities, and an edit prompt yields a diff", async () => {
    const api = new MockApi();
    const session = await api.spawnSession("p-arcade", { tool: "codex", title: "", model: "gpt-5.5", effort: "high", ultracode: false, view: "chat" });
    const first = await api.structuredBacklog(session.id);
    expect(first.capabilities).toEqual(STRUCTURED);
    expect(first.earlier).toBe(false);
    await api.structuredPrompt(session.id, "edit the notes");
    const entry = (await api.structuredBacklog(session.id)).transcript.find((e) => e.kind === "tool");
    expect(entry).toMatchObject({ title: "Edit notes.txt", status: "done", detail: "" });
    expect(entry?.kind === "tool" ? entry.diffs : []).toHaveLength(1);
  });

  it("spawnSession names an untitled task and rejects bad requests", async () => {
    const api = new MockApi();
    const ok = { tool: "claude-code", title: "   ", model: "claude-opus-5-5", effort: "high", ultracode: false, view: "terminal" } as const;
    await expect(api.spawnSession("p-arcade", ok)).resolves.toMatchObject({ title: "New task" });
    await expect(api.spawnSession("nope", ok)).rejects.toThrow(/unknown project/);
    await expect(api.spawnSession("p-arcade", { ...ok, model: "gpt-5.5" })).rejects.toThrow(/not a model the app can start/);
    await expect(api.spawnSession("p-arcade", { ...ok, tool: "opencode" })).rejects.toThrow(/not a model the app can start/);
    await expect(api.spawnSession("p-arcade", { ...ok, tool: "codex", model: "gpt-5.5", effort: "max" })).rejects.toThrow(/does not support max effort/);
  });

  it("hireOptions follows the Agents setting where chat is available", async () => {
    const api = new MockApi();
    expect((await api.hireOptions()).every((o) => o.view === "terminal")).toBe(true);
    await api.updateSettings({ agents: { view: "chat" } });
    const views = Object.fromEntries((await api.hireOptions()).map((o) => [o.tool, o.view]));
    expect(views).toEqual({ "claude-code": "chat", codex: "chat", antigravity: "terminal", opencode: "terminal", cursor: "terminal" });
  });

  it("hires chat desks by whether the tool keeps a terminal, and refuses chat where it is unavailable", async () => {
    const api = new MockApi();
    const claude = { tool: "claude-code", title: "a", model: "claude-opus-5-5", effort: "high", ultracode: false, view: "chat" } as const;
    const full = await api.spawnSession("p-arcade", claude);
    expect(full.control).toBe("full");
    expect((await api.getSnapshot()).chatDesks).toEqual([full.id]);
    const codex = await api.spawnSession("p-arcade", { ...claude, tool: "codex", model: "gpt-5.5" });
    expect(codex.control).toBe("structured");
    expect((await api.getSnapshot()).chatDesks).toEqual([full.id]);
    await expect(api.spawnSession("p-arcade", { ...claude, tool: "antigravity", model: "Gemini 3.6 Flash" })).rejects.toThrow(
      "chat for Antigravity is not built yet, so it runs in the terminal",
    );
  });

  it("keeps a message sent to a desk with an open approval in snapshot.queued until the turn ends", async () => {
    const api = new MockApi();
    const desk = await api.spawnSession("p-arcade", { tool: "codex", title: "", model: "gpt-5.5", effort: "high", ultracode: false, view: "chat" });
    await api.structuredPrompt(desk.id, "run it");
    await api.structuredPrompt(desk.id, "then this");
    const waiting = (await api.getSnapshot()).queued;
    expect(waiting).toEqual([expect.objectContaining({ toSessionId: desk.id, text: "then this", hold: "waiting-on-you" })]);
    const approval = (await api.structuredBacklog(desk.id)).approval;
    await api.structuredAnswer(desk.id, approval?.id ?? "", "deny", {});
    expect((await api.getSnapshot()).queued).toEqual([]);
    expect((await api.structuredBacklog(desk.id)).transcript.map((e) => (e.kind === "agent" ? e.text : ""))).toContain("Done: then this");
  });

  it("withdrawMessage removes a waiting message, and rejects the second time", async () => {
    const api = new MockApi();
    const desk = await api.spawnSession("p-arcade", { tool: "codex", title: "", model: "gpt-5.5", effort: "high", ultracode: false, view: "chat" });
    await api.structuredPrompt(desk.id, "run it");
    await api.structuredPrompt(desk.id, "never mind");
    const id = (await api.getSnapshot()).queued[0]?.id ?? "";
    await api.withdrawMessage(id);
    expect((await api.getSnapshot()).queued).toEqual([]);
    await expect(api.withdrawMessage(id)).rejects.toThrow("that message is no longer waiting");
  });

  it("gives a Claude Code chat desk the terminal capabilities and a card a line break in its terminal closes", async () => {
    const api = new MockApi();
    const claude = { tool: "claude-code", title: "", model: "claude-opus-5-5", effort: "high", ultracode: false, view: "chat" } as const;
    const desk = await api.spawnSession("p-arcade", claude);
    expect((await api.structuredBacklog(desk.id)).capabilities).toEqual({ compose: true, stop: true, end: false, approvals: "terminal", terminal: true });
    await api.structuredPrompt(desk.id, "run the tests");
    expect((await api.structuredBacklog(desk.id)).approval).toEqual({
      id: "terminal-prompt",
      title: "Claude Code is waiting for your answer",
      detail: "Claude needs your permission to use Bash",
      fields: [],
      choices: [],
    });
    await api.writeTerminal(desk.id, "1");
    expect((await api.structuredBacklog(desk.id)).approval).not.toBeNull();
    await api.writeTerminal(desk.id, "\r");
    expect((await api.structuredBacklog(desk.id)).approval).toBeNull();
    await api.structuredCancel(desk.id);
    expect((await api.structuredBacklog(desk.id)).transcript.at(-1)).toMatchObject({ kind: "notice", text: "Turn stopped." });
    await expect(api.structuredStop(desk.id)).rejects.toThrow("only a structured desk can be ended from its conversation");
  });

  it("answerQuestion (option) clears the question, unblocks the session and records a decision", async () => {
    const api = new MockApi(makeSeedSnapshot(), fixedNow);
    await api.answerQuestion("q-1", { kind: "option", optionId: "o-3" });
    const snap = await api.getSnapshot();
    expect(snap.questions.find((q) => q.id === "q-1")).toBeUndefined();
    const s2 = snap.sessions.find((s) => s.id === "s-2");
    expect(s2?.state).toBe("running");
    expect(s2?.pendingQuestionIds).toEqual([]);
    const decision = snap.decisions.at(-1);
    expect(decision).toMatchObject({ projectId: "p-shop", title: "Schema bump (Zod, Pydantic and parity)", by: "human", decidedAt: NOW.toISOString() });
  });

  it("answerQuestion keeps a session blocked while it has other questions", async () => {
    const seed = makeSeedSnapshot();
    const api = new MockApi({
      ...seed,
      questions: [
        ...seed.questions,
        { id: "q-9", projectId: "p-shop", sessionId: "s-2", prompt: "Also bump the version?", options: [], allowOther: true, answerVia: "app", priority: 3, askedAt: "2026-09-28T10:00:00Z", context: "", sourceFile: null },
      ],
      sessions: seed.sessions.map((s) => (s.id === "s-2" ? { ...s, pendingQuestionIds: ["q-1", "q-9"] } : s)),
    });
    await api.answerQuestion("q-1", { kind: "other", text: "prefill only" });
    const s2 = (await api.getSnapshot()).sessions.find((s) => s.id === "s-2");
    expect(s2?.state).toBe("waiting-human");
    expect(s2?.pendingQuestionIds).toEqual(["q-9"]);
  });

  it("answerQuestion records free text as the decision title", async () => {
    const api = new MockApi();
    await api.answerQuestion("q-1", { kind: "other", text: "  prefill only for now " });
    expect((await api.getSnapshot()).decisions.at(-1)?.title).toBe("prefill only for now");
  });

  it("answerQuestion refuses questions that must be answered in the agent's own terminal", async () => {
    const api = new MockApi();
    await expect(api.answerQuestion("q-2", { kind: "option", optionId: "o-1" })).rejects.toThrow(/its own terminal/);
  });

  it("answerQuestion rejects empty free text, unknown option, unknown id", async () => {
    const api = new MockApi();
    await expect(api.answerQuestion("q-1", { kind: "other", text: "   " })).rejects.toThrow(/empty/);
    await expect(api.answerQuestion("q-1", { kind: "option", optionId: "bogus" })).rejects.toThrow(/unknown option/);
    await expect(api.answerQuestion("ghost", { kind: "option", optionId: "o-3" })).rejects.toThrow(/unknown question/);
  });

  it("answerQuestion records the decision on the question's floor, even for a file call with no asker", async () => {
    const api = new MockApi();
    await api.answerQuestion("q-3", { kind: "option", optionId: "o-5" });
    const snap = await api.getSnapshot();
    expect(snap.questions.find((q) => q.id === "q-3")).toBeUndefined();
    expect(snap.decisions.at(-1)).toMatchObject({ projectId: "p-office", title: "Read TODO.md, FOUNDER_TODO.md and FOUNDER_DECISIONS.md" });
  });

  it("tickTodo marks done and rejects unknown", async () => {
    const api = new MockApi();
    await api.tickTodo("t-1");
    const snap = await api.getSnapshot();
    expect(snap.todos.find((t) => t.id === "t-1")?.done).toBe(true);
    await expect(api.tickTodo("t-nope")).rejects.toThrow(/unknown todo/);
  });

  it("sendAgentMessage records the note and validates its ends and text", async () => {
    const api = new MockApi(makeSeedSnapshot(), fixedNow);
    await api.sendAgentMessage("s-1", "s-2", " need your fetcher types ");
    expect((await api.getSnapshot()).messages).toEqual([
      { id: expect.any(String), fromSessionId: "s-1", toSessionId: "s-2", text: "need your fetcher types", sentAt: NOW.toISOString() },
    ]);
    await expect(api.sendAgentMessage("s-1", "ghost", "hi")).rejects.toThrow(/unknown session/);
    await expect(api.sendAgentMessage("s-1", "s-1", "hi")).rejects.toThrow(/itself/);
    await expect(api.sendAgentMessage("s-1", "s-2", "  ")).rejects.toThrow(/empty/);
  });

  it("keeps only the latest 20 messages", async () => {
    const api = new MockApi();
    for (let i = 0; i < 25; i++) await api.sendAgentMessage("s-1", "s-2", `note ${i}`);
    const messages = (await api.getSnapshot()).messages;
    expect(messages).toHaveLength(20);
    expect(messages[0]?.text).toBe("note 5");
  });

  it("addFloor adds an empty floor once, and rejects relative paths", async () => {
    const api = new MockApi();
    const p = await api.addFloor(" ~/code/newapp/ ");
    expect(p).toMatchObject({ name: "newapp", path: "~/code/newapp", sessionIds: [] });
    await expect(api.addFloor("~/code/newapp")).resolves.toEqual(p);
    expect((await api.getSnapshot()).projects.filter((x) => x.path === "~/code/newapp")).toHaveLength(1);
    await expect(api.addFloor("code/app")).rejects.toThrow(/full path/);
    expect(api.canPickFolder).toBe(false);
    await expect(api.pickFolder()).resolves.toBeNull();
  });

  it("focusSession only raises windows of discovered sessions", async () => {
    const api = new MockApi();
    await expect(api.focusSession("s-3")).resolves.toBeUndefined();
    await expect(api.focusSession("s-1")).rejects.toThrow(/no window to raise/);
    await expect(api.focusSession("ghost")).rejects.toThrow(/unknown session/);
  });
});

describe("MockApi simulation", () => {
  it("moves a working session to its next state", async () => {
    const api = new MockApi(makeSeedSnapshot(), fixedNow);
    // roll < 0.7 → state change; second value picks the first movable session (s-1, running).
    api.simulateTick(sequence(0.1, 0));
    const s1 = (await api.getSnapshot()).sessions.find((s) => s.id === "s-1");
    expect(s1).toMatchObject({ state: "thinking", lastActivityAt: NOW.toISOString() });
  });

  it("has a colleague on the same floor send a note", async () => {
    const api = new MockApi();
    // roll ≥ 0.7 → message; s-1 (first awake) writes to its only awake colleague s-2.
    api.simulateTick(sequence(0.9, 0, 0, 0));
    const [m] = (await api.getSnapshot()).messages;
    expect(m).toMatchObject({ fromSessionId: "s-1", toSessionId: "s-2" });
  });

  it("does nothing when nobody can move or talk", async () => {
    const seed = makeSeedSnapshot();
    const quiet = { ...seed, sessions: seed.sessions.map((s) => ({ ...s, state: "done" as const })) };
    const api = new MockApi(quiet);
    const listener = vi.fn();
    api.subscribe(listener);
    api.simulateTick(sequence(0.1, 0));
    api.simulateTick(sequence(0.9, 0));
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("does not send a note when the sender has no awake colleague", async () => {
    const seed = makeSeedSnapshot();
    const api = new MockApi({ ...seed, sessions: seed.sessions.filter((s) => s.id === "s-1") });
    api.simulateTick(sequence(0.9, 0, 0, 0));
    expect((await api.getSnapshot()).messages).toEqual([]);
  });

  it("startSimulation ticks on an interval until stopped", async () => {
    vi.useFakeTimers();
    const api = new MockApi();
    const listener = vi.fn();
    api.subscribe(listener);
    const stop = api.startSimulation(sequence(0.1, 0), 1000);
    vi.advanceTimersByTime(3000);
    stop();
    vi.advanceTimersByTime(3000);
    expect(listener).toHaveBeenCalledTimes(4);
    vi.useRealTimers();
  });
});

describe("MockApi terminals", () => {
  function watch(api: MockApi, sessionId: string): { output: string[]; errors: string[]; stop: () => void } {
    const output: string[] = [];
    const errors: string[] = [];
    const stop = api.subscribeTerminal(sessionId, { output: (d) => output.push(d), exit: () => {}, error: (m) => errors.push(m) });
    return { output, errors, stop };
  }

  it("a fake shell echoes typing, answers each line and replays on reattach", async () => {
    const api = new MockApi();
    const first = watch(api, "s-1");
    expect(first.output).toEqual(["Demo terminal for “audit checkout”. Type a line and press Enter.\r\n$ "]);
    await api.writeTerminal("s-1", "lz\u007fs\u001b\r");
    expect(first.output.slice(1)).toEqual(["lz\b \bs\r\necho: ls\r\n$ "]);
    await api.writeTerminal("s-1", "\u007f\n");
    expect(first.output.at(-1)).toBe("\r\n$ ");
    first.stop();
    await api.writeTerminal("s-1", "x");
    expect(first.output).toHaveLength(3);
    const again = watch(api, "s-1");
    expect(again.output).toEqual([`${first.output.join("")}x`]);
    await expect(api.resizeTerminal("s-1", 120, 30)).resolves.toBeUndefined();
  });

  it("only app-controlled sessions have a terminal", async () => {
    const api = new MockApi();
    expect(watch(api, "s-3").errors).toEqual(["map-tiles mirror was not started in the app, so it has no terminal here"]);
    expect(watch(api, "nope").errors).toEqual(["unknown session: nope"]);
    await expect(api.writeTerminal("s-5", "x")).rejects.toThrow(/no terminal here/);
    await expect(api.resizeTerminal("nope", 80, 24)).rejects.toThrow(/unknown session/);
    await expect(api.resizeTerminal("s-1", 0, 24)).rejects.toThrow(/at least one row and one column/);
  });

  it("demo hires never fail after starting", () => {
    const reports = vi.fn();
    const off = new MockApi().subscribeHireFailures(reports);
    off();
    expect(reports).not.toHaveBeenCalled();
  });
});

describe("makeSeedSnapshot", () => {
  it("is internally consistent (every reference resolves)", () => {
    const snap = makeSeedSnapshot();
    const sessionIds = new Set(snap.sessions.map((s) => s.id));
    const projectIds = new Set(snap.projects.map((p) => p.id));
    for (const p of snap.projects) for (const sid of p.sessionIds) expect(sessionIds.has(sid)).toBe(true);
    for (const s of snap.sessions) expect(projectIds.has(s.projectId)).toBe(true);
    for (const q of snap.questions) {
      expect(projectIds.has(q.projectId)).toBe(true);
      expect(q.sourceFile !== null || sessionIds.has(q.sessionId)).toBe(true);
    }
    for (const d of snap.decisions) expect(projectIds.has(d.projectId)).toBe(true);
    for (const t of snap.todos) expect(projectIds.has(t.projectId)).toBe(true);
  });
});

describe("MockApi floors", () => {
  it("demolishing a floor takes its agents, sign-out sheet and questions with it", async () => {
    const api = new MockApi();
    await api.demolishFloor("p-shop");
    const after = await api.getSnapshot();
    expect(after.projects.map((p) => p.id)).not.toContain("p-shop");
    expect(after.sessions.some((s) => s.projectId === "p-shop")).toBe(false);
    expect(after.ended.some((e) => e.projectId === "p-shop")).toBe(false);
    expect(after.questions.some((q) => q.projectId === "p-shop")).toBe(false);
    await expect(api.demolishFloor("p-shop")).rejects.toThrow(/not a floor/);
  });

  describe("editors", () => {
    const place = { kind: "session", sessionId: "s-1" } as const;

    it("lists the four editors, the default being the first installed", async () => {
      const list = await new MockApi().editors();
      expect(list.editors.map((o) => [o.editor, o.installed])).toEqual([["cursor", true], ["vs-code", true], ["zed", false], ["devin-desktop", false]]);
      expect(list.editors.map((o) => o.label)).toEqual(["Cursor", "VS Code", "Zed", "Devin Desktop"]);
      expect(list.defaultEditor).toBe("cursor");
    });

    it("prefers the saved editor when installed, skips it when not", async () => {
      const api = new MockApi();
      await api.updateSettings({ editor: { preferred: "vs-code" } });
      expect((await api.editors()).defaultEditor).toBe("vs-code");
      await api.updateSettings({ editor: { preferred: "zed" } });
      expect((await api.editors()).defaultEditor).toBe("cursor");
    });

    it("has no default when nothing is installed", async () => {
      const api = new MockApi();
      api.setInstalledEditors([]);
      expect((await api.editors()).defaultEditor).toBeNull();
      await expect(api.openInEditor(place, null)).rejects.toThrow("no editor was found on this Mac (looked for Cursor, VS Code, Zed and Devin Desktop)");
    });

    it("opens each kind of place and logs it", async () => {
      const api = new MockApi();
      const helper = (await api.getSnapshot()).sessions.find((s) => s.id === "s-1")?.helpers[0]?.id ?? "";
      const places = [place, { kind: "helper", sessionId: "s-1", helperId: helper }, { kind: "floor", projectId: "p-shop" }] as const;
      for (const p of places) await expect(api.openInEditor(p, null)).resolves.toBe("cursor");
      await expect(api.openInEditor(place, "vs-code")).resolves.toBe("vs-code");
      expect(api.opened).toEqual([...places.map((p) => ({ place: p, editor: "cursor" })), { place, editor: "vs-code" }]);
    });

    it("opens a signed-out session", async () => {
      const api = new MockApi();
      const ended = (await api.getSnapshot()).ended[0];
      if (ended === undefined) throw new Error("no ended session in the seed");
      await expect(api.openInEditor({ kind: "session", sessionId: ended.id }, null)).resolves.toBe("cursor");
    });

    it("refuses what the backend refuses", async () => {
      const api = new MockApi();
      await expect(api.openInEditor({ kind: "floor", projectId: "nope" }, null)).rejects.toThrow("nope is not a floor of this building");
      await expect(api.openInEditor({ kind: "session", sessionId: "nope" }, null)).rejects.toThrow("unknown session: nope");
      await expect(api.openInEditor({ kind: "helper", sessionId: "nope", helperId: "h" }, null)).rejects.toThrow("unknown session: nope");
      await expect(api.openInEditor({ kind: "helper", sessionId: "s-1", helperId: "nope" }, null)).rejects.toThrow("unknown helper: nope");
      await expect(api.openInEditor(place, "zed")).rejects.toThrow("Zed is not installed on this Mac");
      expect(api.opened).toEqual([]);
    });

    it("says a lost folder is gone", async () => {
      const api = new MockApi();
      api.loseFolder("s-2");
      await expect(api.openInEditor({ kind: "session", sessionId: "s-2" }, null)).rejects.toThrow("that worktree is gone");
      await expect(api.openInEditor(place, null)).resolves.toBe("cursor");
    });
  });

  describe("settings", () => {
    it("starts on the defaults", async () => {
      const settings = await new MockApi().getSettings();
      expect(settings).toEqual({
        notifications: { finished: true, needsYou: true, stuck: true, sound: true, dockBounce: true, systemNotification: true, cooldownSecs: 10 },
        editor: { preferred: null },
        connectors: { linearAgentLabel: "agent" },
        qa: { defaultOn: false, pollMinutes: 5, everyPush: true, skipDrafts: true, postWhenClean: true, maxConcurrent: 1, reviewsPerDay: 3, tool: "claude-code", model: null, effort: null },
        manager: { tool: "claude-code", model: null, effort: null, maxDesks: 3, spendCeilingUsd: 20 },
        agents: { view: "terminal" },
        routing: {
          enabled: false,
          contexts: [],
          connections: [],
          tiers: { fable: [], opus: [], sonnet: [], haiku: [] },
        },
      });
    });

    it("saves a routing group and reads it back", async () => {
      const api = new MockApi();
      const routing = addContext(makeSettings().routing, "ctx-1a2b3c4d", "Work");
      await expect(api.updateSettings({ routing })).resolves.toEqual({ ...makeSettings(), routing });
      await expect(api.getSettings()).resolves.toMatchObject({ routing });
    });

    it("merges an update, returns it and reads it back", async () => {
      const api = new MockApi();
      const notifications = { ...makeSettings().notifications, sound: false, cooldownSecs: 600 };
      const expected = { ...makeSettings(), notifications };
      await expect(api.updateSettings({ notifications })).resolves.toEqual(expected);
      await expect(api.getSettings()).resolves.toEqual(expected);
    });

    it("resolves an empty patch and changes nothing", async () => {
      const api = new MockApi();
      await expect(api.updateSettings({})).resolves.toEqual(makeSettings());
    });

    it.each([601, -1, 1.5])("rejects a cooldown of %s and changes nothing", async (cooldownSecs) => {
      const api = new MockApi();
      await expect(api.updateSettings({ notifications: { ...makeSettings().notifications, cooldownSecs } })).rejects.toThrow(
        "the cooldown must be between 0 and 600 seconds",
      );
      await expect(api.getSettings()).resolves.toEqual(makeSettings());
    });

    it("records the open desk and the test alerts", async () => {
      const api = new MockApi();
      expect(api.openDesk).toBeNull();
      expect(api.testAlerts).toBe(0);
      await api.setOpenDesk("s-1");
      await api.testAlert();
      await api.testAlert();
      expect(api.openDesk).toBe("s-1");
      expect(api.testAlerts).toBe(2);
      await api.setOpenDesk(null);
      expect(api.openDesk).toBeNull();
    });
  });
});
