import { describe, expect, it } from "vitest";
import {
  STATE_LABEL,
  SESSION_STATE_SEVERITY,
  TOOL_LABEL,
  decisionsForProject,
  floorLight,
  formatTokens,
  formatUsd,
  isActive,
  helpersLine,
  modelLine,
  matchesQuery,
  openCount,
  projectCounts,
  questionQueue,
  questionsForProject,
  searchProjects,
  searchSessions,
  sessionById,
  sessionCounts,
  sessionMatches,
  sessionsForProject,
  todosForProject,
  totalSpend,
} from "./selectors";
import { makeSeedSnapshot } from "../api/mockData";
import type { Question, Session, SessionState } from "../types";

function session(state: SessionState, over: Partial<Session> = {}): Session {
  return {
    id: over.id ?? `s-${state}`,
    tool: over.tool ?? "claude-code",
    projectId: over.projectId ?? "p",
    title: over.title ?? "t",
    state,
    control: over.control ?? "full",
    model: over.model ?? null,
    effort: over.effort ?? null,
    lastActivityAt: over.lastActivityAt ?? "2026-09-28T00:00:00Z",
    pendingQuestionIds: over.pendingQuestionIds ?? [],
    spend: over.spend ?? { usd: 0, tokens: 0, unpricedTokens: 0 },
    helpers: over.helpers ?? [],
  };
}

function question(id: string, priority: number, askedAt: string): Question {
  return { id, projectId: "p", sessionId: "s-1", prompt: "?", options: [], allowOther: true, answerVia: "app", priority, askedAt, context: "", sourceFile: null };
}

describe("floorLight", () => {
  it("is off for an empty floor and for a floor where nobody is working", () => {
    expect(floorLight([])).toBe("off");
    expect(floorLight([session("done")])).toBe("off");
    expect(floorLight([session("idle"), session("idle")])).toBe("off");
  });
  it("is green when someone is working and nobody needs you", () => {
    expect(floorLight([session("running"), session("idle"), session("done")])).toBe("green");
    expect(floorLight([session("thinking"), session("idle")])).toBe("green");
  });
  it("is yellow when any session waits on a human", () => {
    expect(floorLight([session("running"), session("waiting-human")])).toBe("yellow");
  });
  it("is red when any session errored (error beats waiting)", () => {
    expect(floorLight([session("waiting-human"), session("error")])).toBe("red");
  });
});

describe("labels", () => {
  it("never expose enum names", () => {
    expect(STATE_LABEL["waiting-human"]).toBe("Needs you");
    expect(STATE_LABEL.error).toBe("Stuck");
    expect(TOOL_LABEL["claude-code"]).toBe("Claude Code");
  });
  it("rank error highest and done lowest", () => {
    expect(SESSION_STATE_SEVERITY.error).toBeGreaterThan(SESSION_STATE_SEVERITY["waiting-human"]);
    expect(SESSION_STATE_SEVERITY.done).toBe(0);
  });
});

describe("counts", () => {
  it("isActive excludes done", () => {
    expect(isActive(session("running"))).toBe(true);
    expect(isActive(session("done"))).toBe(false);
  });
  it("sessionCounts splits active vs total", () => {
    expect(sessionCounts([session("running"), session("done")])).toEqual({ active: 1, total: 2 });
  });
  it("projectCounts and sessionsForProject read from the snapshot", () => {
    const snap = makeSeedSnapshot();
    const shop = snap.projects.find((p) => p.id === "p-shop");
    if (shop === undefined) throw new Error("seed lost shop");
    expect(sessionsForProject(snap, "p-shop")).toHaveLength(3);
    // Only the running agent is working; one waits on you and one is stuck.
    expect(projectCounts(snap, shop)).toEqual({ active: 1, total: 3 });
  });
  it("openCount counts unticked items", () => {
    const snap = makeSeedSnapshot();
    expect(openCount(todosForProject(snap, "p-office", "ai"))).toBe(1);
  });
});

describe("search", () => {
  const snap = makeSeedSnapshot();
  it("matchesQuery is case-insensitive over name and path, and empty matches all", () => {
    const shop = snap.projects.find((p) => p.id === "p-shop");
    if (shop === undefined) throw new Error("seed lost shop");
    expect(matchesQuery(shop, "")).toBe(true);
    expect(matchesQuery(shop, "CORNER")).toBe(true);
    expect(matchesQuery(shop, "~/code/cor")).toBe(true);
    expect(matchesQuery(shop, "zzz")).toBe(false);
  });
  it("searchProjects filters", () => {
    expect(searchProjects(snap.projects, "office").map((p) => p.id)).toEqual(["p-office"]);
    expect(searchProjects(snap.projects, "")).toHaveLength(snap.projects.length);
  });
  it("sessions match on title, tool label and state words", () => {
    expect(searchSessions(snap.sessions, "antigravity").map((s) => s.id)).toEqual(["s-5"]);
    expect(searchSessions(snap.sessions, "audit").map((s) => s.id)).toEqual(["s-1"]);
    expect(searchSessions(snap.sessions, "needs you").map((s) => s.id)).toEqual(["s-2"]);
    expect(searchSessions(snap.sessions, "")).toHaveLength(snap.sessions.length);
    expect(sessionMatches(session("idle"), "zzz")).toBe(false);
  });
});

describe("question queues", () => {
  it("sorts most-urgent (lowest priority) first", () => {
    expect(questionQueue(makeSeedSnapshot()).map((q) => q.id)).toEqual(["q-2", "q-3", "q-1"]);
  });
  it("breaks priority ties by askedAt", () => {
    const snap = {
      ...makeSeedSnapshot(),
      questions: [question("qb", 0, "2026-09-28T10:00:00Z"), question("qa", 0, "2026-09-28T09:00:00Z")],
    };
    expect(questionQueue(snap).map((q) => q.id)).toEqual(["qa", "qb"]);
  });
  it("questionsForProject keeps only the floor's own questions", () => {
    const snap = makeSeedSnapshot();
    expect(questionsForProject(snap, "p-shop").map((q) => q.id)).toEqual(["q-2", "q-1"]);
    expect(questionsForProject(snap, "p-office").map((q) => q.id)).toEqual(["q-3"]);
    expect(questionsForProject(snap, "p-notes")).toEqual([]);
  });
});

describe("per-floor manager data", () => {
  const snap = makeSeedSnapshot();
  it("decisionsForProject is scoped and newest first", () => {
    expect(decisionsForProject(snap, "p-office").map((d) => d.id)).toEqual(["d-2", "d-1"]);
    expect(decisionsForProject(snap, "p-notes")).toEqual([]);
  });
  it("todosForProject is scoped by floor and assignee, open items first", () => {
    expect(todosForProject(snap, "p-office", "ai").map((t) => t.id)).toEqual(["t-4", "t-3"]);
    expect(todosForProject(snap, "p-office", "human").map((t) => t.id)).toEqual(["t-1", "t-2"]);
    expect(todosForProject(snap, "p-shop", "human").map((t) => t.id)).toEqual(["t-5"]);
  });
  it("sessionById finds or misses", () => {
    expect(sessionById(snap, "s-3")?.title).toBe("map-tiles mirror");
    expect(sessionById(snap, "ghost")).toBeUndefined();
  });
});

describe("helpersLine", () => {
  it("counts a session's working helpers, and says nothing when it has none", () => {
    expect(helpersLine(session("running"))).toBeNull();
    expect(helpersLine(session("running", { helpers: [{ id: "a", state: "running" }] }))).toBe("1 helper working");
    expect(helpersLine(session("idle", { helpers: [{ id: "a", state: "running" }, { id: "b", state: "thinking" }] }))).toBe("2 helpers working");
  });
});

describe("modelLine", () => {
  it("reads the model and effort, or nothing when unknown", () => {
    expect(modelLine(session("idle", { model: "claude-opus-5-5", effort: "high" }))).toBe("claude-opus-5-5, high effort");
    expect(modelLine(session("idle", { model: "gpt-5.5" }))).toBe("gpt-5.5");
    expect(modelLine(session("idle"))).toBeNull();
  });
});

describe("spend", () => {
  it("totalSpend sums usd and tokens", () => {
    const total = totalSpend(makeSeedSnapshot().sessions);
    expect(total.usd).toBeCloseTo(1.82 + 0.44 + 0.12 + 2.05 + 0 + 3.7, 5);
    expect(total.tokens).toBe(412_000 + 96_000 + 22_000 + 508_000 + 0 + 910_000);
    expect(total.unpricedTokens).toBe(0);
  });
  it("formats money and tokens", () => {
    expect(formatUsd(2.375)).toBe("$2.38");
    expect(formatTokens(530_000)).toBe("530k tokens");
    expect(formatTokens(1_948_000)).toBe("1.9M tokens");
    expect(formatTokens(900)).toBe("900 tokens");
  });
});
