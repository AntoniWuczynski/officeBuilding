import { describe, expect, it } from "vitest";
import type { BackgroundTask, BlockWhy, DeskHandoverState, DeskWork, LimitWindow, OfferWhy } from "../types";
import {
  blockSentence,
  cardLine,
  confirmSentence,
  endsSentence,
  offerWhySentence,
  otherTool,
  resetSentence,
  signOutLine,
  stripLine,
  tagWord,
  usedUpSentence,
  windowWords,
} from "./handover";

// Local times throughout, because `clock` formats in the local zone.
const NOW = new Date(2026, 10, 13, 12, 0, 0);
const at = (day: number, hour: number, minute: number): number => Math.floor(new Date(2026, 10, day, hour, minute, 0).getTime() / 1000);
const SOON = at(13, 16, 53);
const LATER = at(18, 9, 0);
const PAST = at(13, 8, 0);

type Offered = Extract<DeskHandoverState, { kind: "offered" }>;
type Blocked = Extract<DeskHandoverState, { kind: "blocked" }>;
const offered = (why: OfferWhy, work: DeskWork = { kind: "clear" }): Offered => ({ kind: "offered", to: "codex", why, work, window: "five-hour", resetAt: SOON });
const blocked = (why: BlockWhy, otherResetAt: number | null = null): Blocked => ({ kind: "blocked", to: "codex", why, window: "five-hour", resetAt: SOON, otherResetAt });
const task = (kind: BackgroundTask["kind"], name: string | null = null): BackgroundTask => ({ kind, name });

describe("otherTool and windowWords", () => {
  it("names the other tool and the window", () => {
    expect(otherTool("claude-code")).toBe("codex");
    expect(otherTool("codex")).toBe("claude-code");
    const cases: readonly [LimitWindow | null, string][] = [
      ["five-hour", "5-hour limit"],
      ["seven-day", "weekly limit"],
      [null, "limit"],
    ];
    for (const [window, words] of cases) expect(windowWords(window)).toBe(words);
  });
});

describe("resetSentence", () => {
  it("covers no reset, past, within a day and later", () => {
    expect(resetSentence("five-hour", null, NOW)).toBe("The reset time is not known.");
    expect(resetSentence("five-hour", PAST, NOW)).toBe("The 5-hour limit should have reset by now.");
    expect(resetSentence("five-hour", SOON, NOW)).toBe("The 5-hour limit resets at 16:53.");
    expect(resetSentence("seven-day", LATER, NOW)).toBe("The weekly limit resets on Wednesday at 09:00.");
    expect(resetSentence(null, SOON, NOW)).toBe("The limit resets at 16:53.");
  });
});

describe("usedUpSentence", () => {
  it("names the subscription of the tool", () => {
    expect(usedUpSentence("claude-code", "five-hour", SOON, NOW)).toBe("The Claude subscription is used up. The 5-hour limit resets at 16:53.");
    expect(usedUpSentence("codex", null, null, NOW)).toBe("The ChatGPT plan is used up. The reset time is not known.");
  });
});

describe("offerWhySentence", () => {
  const cases: readonly [OfferWhy, string][] = [
    [{ kind: "automatic-off" }, "Automatic handover is off, so it waits for you."],
    [{ kind: "work-running" }, "It has background work running, so it was not handed over by itself."],
    [{ kind: "work-unknown" }, "The building cannot tell whether it has a workflow or other background work running, so it was not handed over by itself."],
    [{ kind: "seen-elsewhere" }, "Another desk on the same login reached the limit. This one has not been refused yet."],
    [{ kind: "not-at-the-time" }, "It was not handed over when it reached the limit, so it waits for you."],
    [{ kind: "failed", reason: "Codex did not start" }, "The last handover did not work: Codex did not start."],
  ];
  it.each(cases)("%j", (why, text) => {
    expect(offerWhySentence(why)).toBe(text);
  });
});

describe("blockSentence", () => {
  it("covers every reason", () => {
    expect(blockSentence(blocked("not-chosen"))).toBe("To hand this desk over, choose what Codex starts with in Settings, under Handover.");
    expect(blockSentence(blocked("both-used-up", at(13, 18, 10)))).toBe("Codex is used up too, until 18:10, so this desk stays.");
    expect(blockSentence(blocked("both-used-up"))).toBe("Codex is used up too, so this desk stays.");
    expect(blockSentence(blocked("already-handed-over"))).toBe("This desk was already handed over to Codex for this limit.");
  });
});

describe("endsSentence", () => {
  it("covers clear, unknown and running work", () => {
    expect(endsSentence({ kind: "clear" }, "claude-code")).toBe("This ends the Claude Code session at this desk.");
    expect(endsSentence({ kind: "unknown" }, "claude-code")).toBe("This ends the Claude Code session at this desk, and with it any workflow or background work it is running.");
    expect(endsSentence({ kind: "running", tasks: [] }, "claude-code")).toContain("any workflow or background work");
  });
  const head = "This ends the Claude Code session at this desk, and with it ";
  const cases: readonly [readonly BackgroundTask[], string][] = [
    [[task("workflow", "nightly")], "the workflow “nightly”."],
    [[task("workflow")], "1 workflow."],
    [[task("workflow"), task("workflow")], "2 workflows."],
    [[task("subagent"), task("subagent")], "2 background subagents."],
    [[task("shell")], "1 background shell."],
    [[task("scheduled")], "1 scheduled wake-up."],
    [[task("other")], "1 other background task."],
    [[task("workflow", "a"), task("shell")], "the workflow “a” and 1 background shell."],
    [[task("workflow", "a"), task("subagent"), task("other")], "the workflow “a”, 1 background subagent and 1 other background task."],
  ];
  it.each(cases)("%j", (tasks, text) => {
    expect(endsSentence({ kind: "running", tasks }, "claude-code")).toBe(`${head}${text}`);
  });
});

describe("confirmSentence", () => {
  it("joins the end, the conversation and the brief", () => {
    expect(confirmSentence(offered({ kind: "automatic-off" }))).toBe(
      "This ends the Claude Code session at this desk. Its conversation stays on disk and can be resumed from the sign-out sheet. Codex starts in the same folder and is told to read a brief: the visible conversation with tool output cut short, and the state of the folder.",
    );
  });
});

describe("stripLine, tagWord, cardLine and signOutLine", () => {
  const states: readonly (readonly [string, DeskHandoverState, { tone: string; text: string }, string | null, string | null, string | null])[] = [
    [
      "offered",
      offered({ kind: "automatic-off" }),
      { tone: "wait", text: "The Claude subscription is used up. The 5-hour limit resets at 16:53. Automatic handover is off, so it waits for you." },
      "used up",
      "The Claude subscription is used up.",
      null,
    ],
    [
      "blocked",
      blocked("already-handed-over"),
      { tone: "fault", text: "The Claude subscription is used up. The 5-hour limit resets at 16:53. This desk was already handed over to Codex for this limit." },
      "used up",
      "The Claude subscription is used up.",
      null,
    ],
    ["underway", { kind: "underway", to: "codex" }, { tone: "wait", text: "Handing this desk over to Codex." }, "handing over", "Being handed over to Codex", null],
    [
      "arrived",
      { kind: "arrived", from: "claude-code", at: at(13, 14, 2), window: "five-hour", resetAt: SOON },
      { tone: "ok", text: "Handed over from Claude Code at 14:02, when the Claude subscription was used up. The 5-hour limit resets at 16:53." },
      "from Claude Code",
      "Handed over from Claude Code at 14:02",
      null,
    ],
    ["left", { kind: "left", to: "codex", at: at(13, 14, 2) }, { tone: "ok", text: "Handed over to Codex at 14:02." }, null, null, "Handed over to Codex at 14:02"],
    [
      "failed",
      { kind: "failed", to: "codex", at: at(13, 14, 2), reason: "Codex was not found" },
      { tone: "fault", text: "Handover to Codex failed at 14:02: Codex was not found." },
      null,
      null,
      "Handover to Codex failed at 14:02: Codex was not found",
    ],
  ];
  it.each(states)("%s", (_name, state, strip, tag, card, signOut) => {
    expect(stripLine(state, NOW)).toEqual(strip);
    expect(tagWord(state)).toBe(tag);
    expect(cardLine(state, NOW)).toBe(card);
    expect(signOutLine(state)).toBe(signOut);
  });
});
