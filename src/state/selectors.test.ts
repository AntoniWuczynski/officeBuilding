import { describe, expect, it } from "vitest";
import type { Connection, FloorLink, TodoItem } from "../types";
import {
  STATE_LABEL,
  connectionFor,
  connectionLine,
  fileSources,
  linkSummary,
  sourceBadge,
  checkoutLine,
  editorMenuItems,
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
  qaFor,
  qaLine,
  reviewLine,
  splitReviewer,
  STAMP_WORDS,
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
import type { EditorList, QaFloor, Question, Review, Session, SessionState } from "../types";

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
    pets: over.pets ?? [],
    worktree: over.worktree ?? false,
    branch: over.branch ?? null,
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
    expect(helpersLine(session("running", { helpers: [{ id: "a", state: "running", worktree: false, branch: null }] }))).toBe("1 helper working");
    expect(helpersLine(session("idle", { helpers: [{ id: "a", state: "running", worktree: false, branch: null }, { id: "b", state: "thinking", worktree: false, branch: null }] }))).toBe("2 helpers working");
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

describe("editor lines", () => {
  const list: EditorList = {
    editors: [
      { editor: "cursor", label: "Cursor", installed: false },
      { editor: "vs-code", label: "VS Code", installed: true },
      { editor: "zed", label: "Zed", installed: true },
      { editor: "devin-desktop", label: "Devin Desktop", installed: true },
    ],
    defaultEditor: "zed",
  };
  it("checkoutLine says what will open", () => {
    expect(checkoutLine({ worktree: true, branch: "fix/orders" })).toBe("its worktree on branch fix/orders");
    expect(checkoutLine({ worktree: true, branch: null })).toBe("its worktree");
    expect(checkoutLine({ worktree: false, branch: "main" })).toBe("its floor's folder");
  });
  it("editorMenuItems lets the backend choose before the list loads", () => {
    expect(editorMenuItems(null)).toEqual([{ editor: null, label: "Open in editor", name: "your editor" }]);
  });
  it("editorMenuItems is empty with nothing installed", () => {
    expect(editorMenuItems({ editors: list.editors.map((o) => ({ ...o, installed: false })), defaultEditor: null })).toEqual([]);
  });
  it("editorMenuItems puts the default first and keeps list order after it", () => {
    expect(editorMenuItems(list)).toEqual([
      { editor: "zed", label: "Open in Zed", name: "Zed" },
      { editor: "vs-code", label: "Open in VS Code", name: "VS Code" },
      { editor: "devin-desktop", label: "Open in Devin Desktop", name: "Devin Desktop" },
    ]);
  });
});

const LINK: FloorLink = { kind: "linear", teamId: "t", teamKey: "ENG", teamName: "Engineering", project: null, label: null };
const NOW = new Date("2026-11-12T10:20:00Z");
const connection = (patch: Partial<Connection>): Connection => ({ projectId: "p-1", link: LINK, fetchedAt: null, error: null, needsKey: false, truncated: false, ...patch });
const todo = (source: TodoItem["source"]): TodoItem => ({ id: "x", projectId: "p-1", text: "x", done: false, assignee: "ai", source });

describe("connector selectors", () => {
  it("finds a floor's connection", () => {
    const snapshot = { ...makeSeedSnapshot(), connections: [connection({})] };
    expect(connectionFor(snapshot, "p-1")?.link).toEqual(LINK);
    expect(connectionFor(snapshot, "p-2")).toBeUndefined();
  });

  it("words a badge for each kind of source", () => {
    expect(sourceBadge({ kind: "app" })).toBeNull();
    expect(sourceBadge({ kind: "file", file: "TODO.md" })).toBe("TODO.md");
    expect(sourceBadge({ kind: "linear", identifier: "ENG-123", url: "https://linear.app/x" })).toBe("Linear \u00b7 ENG-123");
  });

  it("lists the files once each, in order", () => {
    const items = [todo({ kind: "file", file: "TODO.md" }), todo({ kind: "app" }), todo({ kind: "file", file: "HUMAN_TODO.md" }), todo({ kind: "file", file: "TODO.md" })];
    expect(fileSources(items)).toEqual(["TODO.md", "HUMAN_TODO.md"]);
  });

  it("describes a link in words", () => {
    expect(linkSummary(LINK)).toBe("ENG \u00b7 Engineering");
    expect(linkSummary({ ...LINK, project: { id: "p", name: "Checkout" } })).toBe("ENG \u00b7 Engineering, project Checkout");
    expect(linkSummary({ ...LINK, label: "bug" })).toBe("ENG \u00b7 Engineering, label bug");
    expect(linkSummary({ ...LINK, project: { id: "p", name: "Checkout" }, label: "bug" })).toBe("ENG \u00b7 Engineering, project Checkout, label bug");
  });

  it("says how a connection stands", () => {
    const at = "2026-11-12T10:15:00Z";
    expect(connectionLine(connection({ needsKey: true, fetchedAt: at }), NOW)).toBe("Linear needs an API key before it can show this floor's issues.");
    expect(connectionLine(connection({}), NOW)).toBe("Checking Linear\u2026");
    expect(connectionLine(connection({ fetchedAt: at }), NOW)).toBe("Linear, checked 5 min ago.");
    expect(connectionLine(connection({ fetchedAt: at, truncated: true }), NOW)).toBe("Linear, checked 5 min ago. Only the first 200 issues are shown.");
    expect(connectionLine(connection({ fetchedAt: at, error: "Linear is unreachable" }), NOW)).toBe("Linear, as of 5 min ago. The last check failed: Linear is unreachable.");
    expect(connectionLine(connection({ error: "the team was deleted" }), NOW)).toBe("Linear could not be checked: the team was deleted.");
  });
});

describe("QA selectors", () => {
  const at = new Date("2026-11-12T10:19:00Z");
  const checkedAt = "2026-11-12T10:15:00Z";
  const floor = (over: Partial<QaFloor> = {}): QaFloor => ({ projectId: "p-shop", enabled: true, status: { kind: "off" }, run: null, reviews: [], stamp: null, ...over });
  const watching = (open: number): QaFloor => floor({ status: { kind: "watching", repo: "example-org/widgets", checkedAt, open } });
  const review = (outcome: Review["outcome"]): Review => ({ repo: "example-org/widgets", number: 1, title: "t", head: "h", at: checkedAt, outcome });

  it("finds a floor's desk, and none for the setup floor", () => {
    const snapshot = makeSeedSnapshot();
    expect(qaFor(snapshot, "p-shop")?.projectId).toBe("p-shop");
    expect(qaFor(snapshot, "p-setup")).toBeUndefined();
  });

  it.each<[string, QaFloor, string]>([
    ["a run", floor({ run: { number: 14, title: "Cache the price list", startedAt: checkedAt, sessionId: null } }), "Reviewing #14 \u201cCache the price list\u201d."],
    ["off", floor(), "Off."],
    ["waiting", floor({ status: { kind: "waiting" } }), "Asking GitHub\u2026"],
    ["blocked", floor({ status: { kind: "blocked", reason: "gh is not signed in" } }), "Cannot review: gh is not signed in."],
    ["none open", watching(0), "Watching example-org/widgets. No open pull requests of yours, checked 4 min ago."],
    ["one open", watching(1), "Watching example-org/widgets. 1 open pull request of yours, checked 4 min ago."],
    ["three open", watching(3), "Watching example-org/widgets. 3 open pull requests of yours, checked 4 min ago."],
  ])("says what the desk is doing: %s", (_name, qa, sentence) => {
    expect(qaLine(qa, at)).toBe(sentence);
  });

  it("puts a run before every status", () => {
    const run = { number: 2, title: "x", startedAt: checkedAt, sessionId: null };
    expect(qaLine({ ...watching(3), run }, at)).toBe("Reviewing #2 \u201cx\u201d.");
  });

  it.each<[Review["outcome"], string]>([
    [{ kind: "posted", verdict: "pass", findings: 0, reviewId: 1 }, "No issues found"],
    [{ kind: "posted", verdict: "concerns", findings: 1, reviewId: 1 }, "1 finding"],
    [{ kind: "posted", verdict: "concerns", findings: 2, reviewId: 1 }, "2 findings"],
    [{ kind: "clean" }, "Nothing to flag, nothing posted"],
    [{ kind: "failed", reason: "the fetch timed out" }, "Not reviewed: the fetch timed out"],
  ])("says how a review ended: %j", (outcome, sentence) => {
    expect(reviewLine(review(outcome))).toBe(sentence);
  });

  it("words the stamps", () => {
    expect(STAMP_WORDS).toEqual({ pass: { label: "QA passed", spoken: "QA passed" }, concerns: { label: "QA notes", spoken: "QA found problems" } });
  });

  describe("splitReviewer", () => {
    const sessions = [session("running", { id: "a" }), session("thinking", { id: "b" }), session("idle", { id: "c" })];
    const running = (sessionId: string | null): QaFloor => floor({ run: { number: 1, title: "t", startedAt: checkedAt, sessionId } });

    it("takes the reviewer out of the desks", () => {
      const split = splitReviewer(sessions, running("b"));
      expect(split.reviewer?.id).toBe("b");
      expect(split.seated.map((s) => s.id)).toEqual(["a", "c"]);
    });

    it("seats everyone when QA is off, the id is not on the floor, or there is no run", () => {
      expect(splitReviewer(sessions, { ...running("b"), enabled: false })).toEqual({ seated: sessions, reviewer: undefined });
      expect(splitReviewer(sessions, running("zzz"))).toEqual({ seated: sessions, reviewer: undefined });
      expect(splitReviewer(sessions, running(null))).toEqual({ seated: sessions, reviewer: undefined });
      expect(splitReviewer(sessions, floor())).toEqual({ seated: sessions, reviewer: undefined });
      expect(splitReviewer(sessions, undefined)).toEqual({ seated: sessions, reviewer: undefined });
    });
  });
});
