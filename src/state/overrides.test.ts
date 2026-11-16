import { describe, expect, it } from "vitest";
import { makeDemoSnapshot } from "../api/mockData";
import { applyOverrides, NO_OVERRIDES, withMove, withName } from "./overrides";

const snap = makeDemoSnapshot();
const [a, b] = snap.projects;
const mover = snap.sessions.find((s) => s.projectId === a?.id);

describe("applyOverrides", () => {
  it("changes nothing without overrides", () => {
    expect(applyOverrides(snap, NO_OVERRIDES)).toBe(snap);
  });

  it("renames an island and nothing else", () => {
    if (a === undefined) throw new Error("demo has a project");
    const out = applyOverrides(snap, withName(NO_OVERRIDES, a.id, "Home base"));
    expect(out.projects.find((p) => p.id === a.id)?.name).toBe("Home base");
    expect(out.projects.find((p) => p.id === a.id)?.path).toBe(a.path);
    expect(out.sessions).toBe(snap.sessions);
  });

  it("a blank name takes the override away", () => {
    if (a === undefined) throw new Error("demo has a project");
    const named = withName(NO_OVERRIDES, a.id, "Home base");
    expect(withName(named, a.id, "  ")).toEqual(NO_OVERRIDES);
  });

  it("moves a session to another island: its project id, both islands' lists and its questions follow", () => {
    if (a === undefined || b === undefined || mover === undefined) throw new Error("demo has two projects and a session");
    const out = applyOverrides(snap, withMove(NO_OVERRIDES, mover.id, b.id));
    expect(out.sessions.find((s) => s.id === mover.id)?.projectId).toBe(b.id);
    expect(out.projects.find((p) => p.id === a.id)?.sessionIds).not.toContain(mover.id);
    expect(out.projects.find((p) => p.id === b.id)?.sessionIds).toContain(mover.id);
    for (const q of out.questions.filter((x) => x.sessionId === mover.id)) expect(q.projectId).toBe(b.id);
    // Nothing else moved.
    expect(out.sessions.filter((s) => s.id !== mover.id)).toEqual(snap.sessions.filter((s) => s.id !== mover.id));
  });

  it("ignores a move to an island that is gone, and moving back clears it", () => {
    if (mover === undefined || b === undefined) throw new Error("demo has a session");
    expect(applyOverrides(snap, withMove(NO_OVERRIDES, mover.id, "no-such-island")).sessions).toEqual(snap.sessions);
    expect(withMove(withMove(NO_OVERRIDES, mover.id, b.id), mover.id, null)).toEqual(NO_OVERRIDES);
  });
});
