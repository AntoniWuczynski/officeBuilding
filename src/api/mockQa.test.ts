import { describe, expect, it } from "vitest";
import { MockApi } from "./mockApi";
import { makeSeedSnapshot } from "./mockData";
import { makeQaSettings } from "./mockQa";

const NOW = new Date("2026-09-28T16:00:00Z");
const api = (): MockApi => new MockApi(makeSeedSnapshot(), () => NOW);

describe("the QA seed", () => {
  it("has one entry per project, in order", () => {
    const snap = makeSeedSnapshot();
    expect(snap.qa.map((q) => q.projectId)).toEqual(snap.projects.map((p) => p.id));
    expect(snap.qa.map((q) => q.status.kind)).toEqual(["watching", "watching", "blocked", "off"]);
    expect(snap.qa[0]?.run?.sessionId).toBe("s-1");
    expect(snap.qa.map((q) => q.stamp)).toEqual(["concerns", "pass", null, null]);
  });
});

describe("MockApi QA", () => {
  const qa = async (a: MockApi, id: string) => (await a.getSnapshot()).qa.find((q) => q.projectId === id);

  it("switches a floor on to waiting and off again, clearing the run and stamp", async () => {
    const a = api();
    await a.setFloorQa("p-notes", true);
    expect(await qa(a, "p-notes")).toMatchObject({ enabled: true, status: { kind: "waiting" } });
    await a.setFloorQa("p-shop", false);
    expect(await qa(a, "p-shop")).toMatchObject({ enabled: false, status: { kind: "off" }, run: null, stamp: null });
  });

  it("gives a floor with no entry one", async () => {
    const a = new MockApi({ ...makeSeedSnapshot(), qa: [] }, () => NOW);
    await a.setFloorQa("p-shop", true);
    expect(await qa(a, "p-shop")).toMatchObject({ enabled: true, status: { kind: "waiting" }, reviews: [] });
  });

  it("rejects an unknown floor", async () => {
    await expect(api().setFloorQa("p-nowhere", true)).rejects.toThrow("p-nowhere is not a floor of this building");
  });

  it("moves a waiting floor to watching on a re-check and keeps the rest", async () => {
    const a = api();
    await a.setFloorQa("p-notes", true);
    await a.recheckQa();
    expect((await qa(a, "p-notes"))?.status).toEqual({ kind: "watching", repo: "example-org/notes", checkedAt: NOW.toISOString(), open: 0 });
    expect((await qa(a, "p-shop"))?.status).toMatchObject({ kind: "watching", open: 2, checkedAt: NOW.toISOString() });
    expect((await qa(a, "p-arcade"))?.status.kind).toBe("blocked");
  });

  it("falls back to the project id for the repo of a floor it does not know", async () => {
    const a = new MockApi({ ...makeSeedSnapshot(), qa: [] }, () => NOW);
    await a.setFloorQa("p-shop", true);
    await a.recheckQa();
    expect((await qa(a, "p-shop"))?.status).toMatchObject({ repo: "example-org/corner_shop" });
  });

  it("logs a known review and rejects an unknown one", async () => {
    const a = api();
    const head = "0123456789abcdef0123456789abcdef01234567";
    await a.openReview("p-shop", 12, head);
    expect(a.openedReviews()).toEqual([`p-shop#12@${head}`]);
    await expect(a.openReview("p-shop", 13, head)).rejects.toThrow("that review is no longer on this floor's record");
    await expect(a.openReview("p-nowhere", 12, head)).rejects.toThrow("no longer on this floor's record");
  });
});

describe("MockApi QA settings", () => {
  it.each([
    [{ pollMinutes: 0 }, "the check interval must be between 1 and 60 minutes"],
    [{ pollMinutes: 61 }, "the check interval must be between 1 and 60 minutes"],
    [{ maxConcurrent: 0 }, "reviews at once must be between 1 and 4"],
    [{ maxConcurrent: 5 }, "reviews at once must be between 1 and 4"],
    [{ reviewsPerDay: 0 }, "reviews a day must be between 1 and 20"],
    [{ reviewsPerDay: 21 }, "reviews a day must be between 1 and 20"],
  ])("refuses %j", async (change, words) => {
    const a = api();
    await expect(a.updateSettings({ qa: { ...makeQaSettings(), ...change } })).rejects.toThrow(words);
    expect((await a.getSettings()).qa).toEqual(makeQaSettings());
  });

  it("accepts the ends of each range", async () => {
    const a = api();
    for (const change of [{ pollMinutes: 1, maxConcurrent: 1, reviewsPerDay: 1 }, { pollMinutes: 60, maxConcurrent: 4, reviewsPerDay: 20 }]) {
      const saved = await a.updateSettings({ qa: { ...makeQaSettings(), ...change } });
      expect(saved.qa).toMatchObject(change);
    }
  });
});
