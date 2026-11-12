import { describe, expect, it } from "vitest";
import { makeSeedSnapshot } from "../api/mockData";
import type { ManagerFloor, ManagerLogEntry, NoteHold, NoteState, Session } from "../types";
import { HOLD_WORDS, NOTE_STATE_WORDS, capsLine, logLine, managerFor, managerLamp, managerLine, notesFor, splitManager } from "./selectors";

const seed = makeSeedSnapshot();
const floor = (id: string): ManagerFloor => {
  const found = managerFor(seed, id);
  if (found === undefined) throw new Error(`no manager on ${id}`);
  return found;
};
const hired = (sessionId: string | null, over: Partial<ManagerFloor> = {}): ManagerFloor => ({
  ...floor("p-shop"),
  status: { kind: "hired", sessionId, startedAt: "2026-11-12T10:00:00Z", lastCallAt: null },
  ...over,
});
const template = seed.sessions[0];
if (template === undefined) throw new Error("the seed has no session");
const session = (id: string, state: Session["state"] = "running"): Session => ({ ...template, id, state });

describe("managerFor and splitManager", () => {
  it("finds a floor's manager", () => {
    expect(managerFor(seed, "p-shop")?.projectId).toBe("p-shop");
    expect(managerFor(seed, "nowhere")).toBeUndefined();
  });

  it("splits the manager's session from the rest", () => {
    const sessions = [session("a"), session("b"), session("c")];
    const split = splitManager(sessions, hired("b"));
    expect(split.managerSession?.id).toBe("b");
    expect(split.rest.map((s) => s.id)).toEqual(["a", "c"]);
  });

  it("leaves everything in the rest with no manager, no session yet, or a session not on the floor", () => {
    const sessions = [session("a")];
    expect(splitManager(sessions, undefined)).toEqual({ rest: sessions, managerSession: undefined });
    expect(splitManager(sessions, floor("p-office"))).toEqual({ rest: sessions, managerSession: undefined });
    expect(splitManager(sessions, hired(null))).toEqual({ rest: sessions, managerSession: undefined });
    expect(splitManager(sessions, hired("gone"))).toEqual({ rest: sessions, managerSession: undefined });
  });
});

describe("words", () => {
  it("has words for every hold and note state", () => {
    const holds: NoteHold[] = ["busy", "waiting-on-you", "stuck", "finished", "you-typed", "awaiting-turn", "hand-started", "no-idle-signal", "paused"];
    for (const h of holds) expect(HOLD_WORDS[h].length).toBeGreaterThan(0);
    expect(HOLD_WORDS.busy).toBe("it is busy");
    expect(HOLD_WORDS["you-typed"]).toBe("you typed there in the last minute");
    expect(HOLD_WORDS["hand-started"]).toBe("it was started outside the building, so nothing is typed into it");
    expect(HOLD_WORDS.paused).toBe("the manager is paused");
    const states: Record<NoteState, string> = {
      queued: "Waiting",
      delivered: "Delivered",
      withdrawn: "Withdrawn by you",
      overridden: "Replaced by your message",
      dropped: "Dropped: the desk or the manager left",
      failed: "Could not be handed over",
    };
    expect(NOTE_STATE_WORDS).toEqual(states);
  });
});

describe("managerLamp and managerLine", () => {
  it("lights the lamp green, yellow or off", () => {
    expect(managerLamp(floor("p-office"))).toBe("done");
    expect(managerLamp(floor("p-arcade"))).toBe("done");
    expect(managerLamp(hired(null))).toBe("running");
    expect(managerLamp(hired(null, { escalated: true }))).toBe("waiting-human");
    expect(managerLamp(hired(null, { escalated: true, paused: true }))).toBe("done");
  });

  it("says in a few words what the manager is doing", () => {
    expect(managerLine(undefined, undefined)).toBe("Nobody is hired");
    expect(managerLine(floor("p-office"), undefined)).toBe("Nobody is hired");
    expect(managerLine(floor("p-arcade"), session("m"))).toBe("Paused");
    expect(managerLine(hired(null), undefined)).toBe("Starting");
    expect(managerLine(hired("m"), session("m", "waiting-human"))).toBe("Needs you");
  });
});

describe("capsLine and notesFor", () => {
  it("words the caps", () => {
    expect(capsLine({ maxDesks: 3, hiredNow: 1, spendCeilingUsd: 20, spentUsd: 4.1, unpricedDesks: 0 })).toBe("1 of 3 desks hired, $4.10 of $20.00 spent");
  });

  it("lists the notes waiting for one desk", () => {
    expect(notesFor(floor("p-shop"), "s-1").map((n) => n.id)).toEqual(["n-2"]);
    expect(notesFor(floor("p-shop"), "s-2")).toEqual([]);
    expect(notesFor(undefined, "s-1")).toEqual([]);
  });
});

describe("logLine", () => {
  const entry = (event: ManagerLogEntry["event"]): ManagerLogEntry => ({ id: "l", at: "2026-11-12T10:00:00Z", event });
  it("words each kind of entry", () => {
    expect(logLine(entry({ kind: "note", noteId: "n", toSessionId: "s", toTitle: "audit", text: "go on", state: "queued" }))).toBe("To “audit”: go on");
    expect(logLine(entry({ kind: "hire", tool: "codex", task: "fix the bug" }))).toBe("Hired Codex for “fix the bug”");
    expect(logLine(entry({ kind: "escalation", title: "Which database?" }))).toBe("Asked you: Which database?");
  });

  it("never shows a tool's raw name in a refusal", () => {
    const refused = (tool: string): string => logLine(entry({ kind: "refused", tool, reason: "too many" }));
    expect(refused("send_to_desk")).toBe("Refused a note: too many");
    expect(refused("hire_desk")).toBe("Refused a hire: too many");
    expect(refused("escalate")).toBe("Refused a question for you: too many");
    expect(refused("read_desk")).toBe("Refused a request: too many");
  });
});
