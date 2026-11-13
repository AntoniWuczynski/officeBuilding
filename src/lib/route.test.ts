import { describe, expect, it } from "vitest";
import { makeDeskRoute } from "../api/mockRouting";
import type { ConnectionKind, ExhaustedScope, RouteEntry, RouteSkip } from "../types";
import {
  bucketLabel,
  canRetry,
  canSwitch,
  clock,
  entryStateSentence,
  problemSentence,
  reasoningSentence,
  routeHeadline,
  scopeSentence,
  servedSentence,
  skipSentence,
  switchSentence,
} from "./route";

const SCOPE: ExhaustedScope = { bucket: { kind: "whole" }, window: null, observedAt: 1700000000, resetAt: null, checking: false, retryArmed: false };
const ENTRY: RouteEntry = { connection: "sub", name: "Login", kind: "claude-subscription", state: { kind: "ready" }, exhausted: [], problem: null };

describe("skipSentence", () => {
  const cases: readonly [RouteSkip, string][] = [
    [{ kind: "removed" }, "it is no longer in Settings"],
    [{ kind: "paid-not-allowed" }, 'it is billed for each request and may not spend money (Settings, Routing, "May spend money")'],
    [{ kind: "experimental-not-allowed" }, "it is an experimental route that is not switched on"],
    [{ kind: "unmapped-model", model: "m-1" }, "the model m-1 is not listed under any tier in Settings"],
    [{ kind: "no-target", tier: "fable" }, "it has no model for the Fable tier"],
    [{ kind: "effort-unmapped", tier: "haiku", effort: "high" }, "it has no mapping for high effort on the Haiku tier"],
    [{ kind: "unsupported", capability: "tools" }, "it cannot carry tools"],
    [{ kind: "unsupported", capability: "images" }, "it cannot carry images"],
    [{ kind: "unsupported", capability: "documents" }, "it cannot carry documents"],
    [{ kind: "unsupported", capability: "server-tools" }, "it cannot carry tools the provider runs"],
    [{ kind: "exhausted" }, "its allowance is used up"],
    [{ kind: "tried" }, "this request already tried it"],
  ];
  it.each(cases)("words %j", (skip, text) => {
    expect(skipSentence("openrouter", skip)).toBe(text);
  });

  it("words a missing credential by whether the build relays the kind", () => {
    const kinds: readonly ConnectionKind[] = ["claude-subscription", "anthropic-api-key", "openrouter"];
    for (const kind of kinds) expect(skipSentence(kind, { kind: "no-credential" })).toBe("no API key is saved for it");
    for (const kind of ["openai-api-key", "chatgpt-sign-in"] as const) {
      expect(skipSentence(kind, { kind: "no-credential" })).toBe("this build cannot use that kind of connection yet");
    }
  });
});

describe("labels and sentences", () => {
  it("names each bucket", () => {
    expect(bucketLabel({ kind: "whole" })).toBe("Everything on this connection");
    expect(bucketLabel({ kind: "tier", tier: "opus" })).toBe("The Opus tier");
    expect(bucketLabel({ kind: "model", model: "m-1" })).toBe("The model m-1");
  });

  it("words each problem", () => {
    expect(problemSentence("throttled")).toBe("the provider is limiting requests for now");
    expect(problemSentence("faulted")).toBe("the provider failed or is overloaded");
    expect(problemSentence("unauthorised")).toBe("the provider refused the credential");
    expect(problemSentence("forbidden")).toBe("the account may not do this, or its billing needs attention");
    expect(problemSentence("rejected")).toBe("the provider refused the request itself");
    expect(problemSentence("unreachable")).toBe("the provider could not be reached");
    expect(problemSentence("interrupted")).toBe("the connection broke after a request was sent");
  });

  it("formats a clock as hours and minutes", () => {
    expect(clock(1700000000)).toMatch(/^\d\d:\d\d$/);
  });

  it("says nothing about intact reasoning and something for the other two", () => {
    expect(reasoningSentence("intact")).toBeNull();
    expect(reasoningSentence("vendor-may-drop")).toContain("Anthropic leaves out whatever the new account or model cannot read");
    expect(reasoningSentence("removed")).toContain("removed from every request it sends through OpenRouter");
  });

  it("describes entry states", () => {
    expect(entryStateSentence({ ...ENTRY, state: { kind: "active" } })).toBe("Serving this desk");
    expect(entryStateSentence(ENTRY)).toBe("Ready");
    expect(entryStateSentence({ ...ENTRY, state: { kind: "unavailable", reason: { kind: "tried" } } })).toBe("Not used: this request already tried it");
  });
});

describe("routeHeadline", () => {
  const base = makeDeskRoute();
  it("is blocked, with a fault lamp", () => {
    expect(routeHeadline({ ...base, status: { ...base.status, blockedSince: 5 } })).toEqual({ text: "Blocked", tone: "fault" });
  });
  it("waits when nothing is active", () => {
    expect(routeHeadline({ ...base, status: { ...base.status, active: null } })).toEqual({ text: "No request yet", tone: "wait" });
  });
  it("names a healthy active connection, and waits on one with an allowance or a problem", () => {
    expect(routeHeadline(base)).toEqual({ text: "Claude login", tone: "ok" });
    const [sub, ...rest] = base.status.entries;
    if (sub === undefined) throw new Error("no entries");
    const with_ = (entry: RouteEntry) => ({ ...base, status: { ...base.status, entries: [entry, ...rest] } });
    expect(routeHeadline(with_({ ...sub, exhausted: [SCOPE] })).tone).toBe("wait");
    expect(routeHeadline(with_({ ...sub, problem: { kind: "faulted", at: 1 } })).tone).toBe("wait");
  });
  it("falls back to the id when the entry is missing", () => {
    expect(routeHeadline({ ...base, status: { ...base.status, active: "gone" } })).toEqual({ text: "gone", tone: "ok" });
  });
});

describe("switchSentence and servedSentence", () => {
  const { status } = makeDeskRoute();
  it("is null with nothing recorded", () => {
    expect(switchSentence(status)).toBeNull();
    expect(servedSentence(status)).toBeNull();
  });
  it("words both switch reasons, with ids for missing entries", () => {
    const at = clock(1700000100);
    expect(switchSentence({ ...status, lastSwitch: { from: "sub", to: "api", reason: "exhausted", at: 1700000100 } })).toBe(
      `Moved from Claude login to Anthropic key at ${at}, because the allowance of Claude login was used up.`,
    );
    expect(switchSentence({ ...status, lastSwitch: { from: "api", to: "zz", reason: "manual", at: 1700000100 } })).toBe(`Switched from Anthropic key to zz at ${at}, by you.`);
  });
  it("words a request sent as asked and one sent as something else", () => {
    const served = { connection: "api", requestedModel: "big", requestedEffort: "high", model: null, effort: "high", at: 1 };
    expect(servedSentence({ ...status, lastServed: served })).toBe("Last request asked for big at high effort and was sent as asked.");
    expect(servedSentence({ ...status, lastServed: { ...served, requestedEffort: null, effort: null } })).toBe("Last request asked for big and was sent as asked.");
    expect(servedSentence({ ...status, lastServed: { ...served, model: "small", effort: "low" } })).toBe("Last request asked for big at high effort and was sent as small at low effort.");
    expect(servedSentence({ ...status, lastServed: { ...served, effort: null } })).toBe("Last request asked for big at high effort and was sent as big.");
  });
});

describe("scopeSentence", () => {
  const head = `Everything on this connection is used up, seen at ${clock(1700000000)}.`;
  it("words the five endings", () => {
    expect(scopeSentence({ ...SCOPE, checking: true }, 0)).toBe(`${head} A request is checking it now.`);
    expect(scopeSentence({ ...SCOPE, retryArmed: true }, 0)).toBe(`${head} A retry is armed: the next request that needs it checks it once.`);
    expect(scopeSentence(SCOPE, 0)).toBe(`${head} The provider gave no reset time, so it stays shut until you retry.`);
    expect(scopeSentence({ ...SCOPE, resetAt: 1700003600 }, 1700000100)).toBe(`${head} The provider says it resets at ${clock(1700003600)}. One check is allowed after that.`);
    expect(scopeSentence({ ...SCOPE, resetAt: 1700003600 }, 1700003600)).toContain("Its reset time has passed.");
  });
  it("names the five-hour window in words and leaves any other provider name out", () => {
    expect(scopeSentence({ ...SCOPE, window: "five_hour" }, 0)).toContain("is used up (the five-hour limit), seen at");
    expect(scopeSentence({ ...SCOPE, window: "seven_day" }, 0)).toContain("is used up, seen at");
  });
});

describe("canRetry and canSwitch", () => {
  it("retries only a scope that is neither checking nor armed", () => {
    expect(canRetry(ENTRY)).toBe(false);
    expect(canRetry({ ...ENTRY, exhausted: [{ ...SCOPE, checking: true }, { ...SCOPE, retryArmed: true }] })).toBe(false);
    expect(canRetry({ ...ENTRY, exhausted: [{ ...SCOPE, checking: true }, SCOPE] })).toBe(true);
  });
  it("switches to a ready entry or one that is only used up", () => {
    expect(canSwitch({ ...ENTRY, state: { kind: "active" } })).toBe(false);
    expect(canSwitch(ENTRY)).toBe(true);
    expect(canSwitch({ ...ENTRY, state: { kind: "unavailable", reason: { kind: "exhausted" } } })).toBe(true);
    expect(canSwitch({ ...ENTRY, state: { kind: "unavailable", reason: { kind: "tried" } } })).toBe(false);
  });
});
