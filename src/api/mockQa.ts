import type { QaFloor, QaSettings } from "../types";
import { between, QA_MAX_CONCURRENT, QA_POLL_MINUTES, QA_REVIEWS_PER_DAY } from "../lib/qaLimits";

/** The demo QA desks, one per seeded floor (invented data). */
export function makeSeedQa(): QaFloor[] {
  return [
    {
      projectId: "p-shop",
      enabled: true,
      status: { kind: "watching", repo: "example-org/corner_shop", checkedAt: "2026-09-28T15:38:00Z", open: 2 },
      run: { number: 14, title: "Cache the price list", startedAt: "2026-09-28T15:39:00Z", sessionId: "s-1" },
      reviews: [
        {
          repo: "example-org/corner_shop",
          number: 12,
          title: "Add a retry to the uploader",
          head: "0123456789abcdef0123456789abcdef01234567",
          at: "2026-09-28T15:05:00Z",
          outcome: { kind: "posted", verdict: "concerns", findings: 2, reviewId: 90210 },
        },
      ],
      stamp: "concerns",
    },
    {
      projectId: "p-office",
      enabled: true,
      status: { kind: "watching", repo: "example-org/office", checkedAt: "2026-09-28T15:38:00Z", open: 1 },
      run: null,
      reviews: [
        {
          repo: "example-org/office",
          number: 7,
          title: "Tidy the floor plan",
          head: "89abcdef0123456789abcdef0123456789abcdef",
          at: "2026-09-28T14:50:00Z",
          outcome: { kind: "posted", verdict: "pass", findings: 0, reviewId: 90211 },
        },
      ],
      stamp: "pass",
    },
    {
      projectId: "p-arcade",
      enabled: true,
      status: { kind: "blocked", reason: "gh is not signed in to GitHub. Run gh auth login in a terminal" },
      run: null,
      reviews: [],
      stamp: null,
    },
    { projectId: "p-notes", enabled: false, status: { kind: "off" }, run: null, reviews: [], stamp: null },
  ];
}

/** The QA group of a fresh install (`QaSettings::default` in settings.rs). */
export function makeQaSettings(): QaSettings {
  return {
    defaultOn: false,
    pollMinutes: 5,
    everyPush: true,
    skipDrafts: true,
    postWhenClean: true,
    maxConcurrent: 1,
    reviewsPerDay: 3,
    tool: "claude-code",
    model: null,
    effort: null,
  };
}

interface Range {
  readonly min: number;
  readonly max: number;
}

const inRange = (n: number, r: Range): boolean => Number.isInteger(n) && n >= r.min && n <= r.max;

/** Why a QA patch would be refused, with the backend's words, or null. */
export function qaPatchError(qa: QaSettings): string | null {
  if (!inRange(qa.pollMinutes, QA_POLL_MINUTES)) return `the check interval must be ${between(QA_POLL_MINUTES)} minutes`;
  if (!inRange(qa.maxConcurrent, QA_MAX_CONCURRENT)) return `reviews at once must be ${between(QA_MAX_CONCURRENT)}`;
  if (!inRange(qa.reviewsPerDay, QA_REVIEWS_PER_DAY)) return `reviews a day must be ${between(QA_REVIEWS_PER_DAY)}`;
  return null;
}

/** The list with a floor's QA switched on or off. A floor with no entry gets one. */
export function withQaSwitch(list: readonly QaFloor[], projectId: string, enabled: boolean): QaFloor[] {
  const entry: QaFloor = { projectId, enabled, status: enabled ? { kind: "waiting" } : { kind: "off" }, run: null, reviews: [], stamp: null };
  if (!list.some((q) => q.projectId === projectId)) return [...list, entry];
  return list.map((q) => {
    if (q.projectId !== projectId) return q;
    return enabled ? { ...q, enabled, status: entry.status } : { ...q, enabled, status: entry.status, run: null, stamp: null };
  });
}

/** The list after GitHub was asked again: every waiting or watching floor was checked at `checkedAt`. */
export function withQaChecked(list: readonly QaFloor[], names: ReadonlyMap<string, string>, checkedAt: string): QaFloor[] {
  return list.map((q) => {
    if (!q.enabled || (q.status.kind !== "waiting" && q.status.kind !== "watching")) return q;
    const open = q.status.kind === "watching" ? q.status.open : 0;
    const repo = q.status.kind === "watching" ? q.status.repo : `example-org/${names.get(q.projectId) ?? q.projectId}`;
    return { ...q, status: { kind: "watching", repo, checkedAt, open } };
  });
}
