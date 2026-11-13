import type { ConnectionKind, DeskRoute, ExhaustedScope, RouteBucket, RouteCapability, RouteEntry, RouteProblemKind, RouteSkip, RouteStatus, TierId } from "../types";
import { TIERS, kindTraits } from "./routing";

// Every sentence the route strip shows. The skip sentences are the same words as `answers::skip_sentence` in the backend.

const RELAYED: readonly ConnectionKind[] = ["claude-subscription", "anthropic-api-key", "openrouter", "openai-api-key"];

const CAPABILITY: Record<RouteCapability, string> = {
  tools: "tools",
  images: "images",
  documents: "documents",
  "server-tools": "tools the provider runs",
};

function tierName(tier: TierId): string {
  const label = TIERS.find((t) => t.id === tier)?.label ?? tier;
  return label.split(" / ")[0] ?? label;
}

export function skipSentence(kind: ConnectionKind, skip: RouteSkip): string {
  switch (skip.kind) {
    case "removed":
      return "it is no longer in Settings";
    case "paid-not-allowed":
      return 'it is billed for each request and may not spend money (Settings, Routing, "May spend money")';
    case "experimental-not-allowed":
      return "it is an experimental route that is not switched on";
    case "no-credential":
      return RELAYED.includes(kind) ? "no API key is saved for it" : "it needs Sign in with ChatGPT, which this build does not have yet";
    case "unmapped-model":
      return `the model ${skip.model} is not listed under any tier in Settings`;
    case "no-target":
      return `it has no model for the ${tierName(skip.tier)} tier`;
    case "effort-unmapped":
      return `it has no mapping for ${skip.effort} effort on the ${tierName(skip.tier)} tier`;
    case "unsupported":
      return `it cannot carry ${CAPABILITY[skip.capability]}`;
    case "exhausted":
      return "its allowance is used up";
    case "tried":
      return "this request already tried it";
  }
}

export function bucketLabel(bucket: RouteBucket): string {
  switch (bucket.kind) {
    case "whole":
      return "Everything on this connection";
    case "tier":
      return `The ${tierName(bucket.tier)} tier`;
    case "model":
      return `The model ${bucket.model}`;
  }
}

const PROBLEM: Record<RouteProblemKind, string> = {
  throttled: "the provider is limiting requests for now",
  faulted: "the provider failed or is overloaded",
  unauthorised: "the provider refused the credential",
  forbidden: "the account may not do this, or its billing needs attention",
  rejected: "the provider refused the request itself",
  unreachable: "the provider could not be reached",
  interrupted: "the connection broke after a request was sent",
};

export function problemSentence(kind: RouteProblemKind): string {
  return PROBLEM[kind];
}

const CLOCK = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit", hour12: false });

/** Local time of a Unix-seconds stamp, hours and minutes. */
export function clock(seconds: number): string {
  return CLOCK.format(seconds * 1000);
}

function nameOf(route: RouteStatus, id: string): string {
  return route.entries.find((e) => e.connection === id)?.name ?? id;
}

/** The strip's one line and its lamp. */
export function routeHeadline(route: DeskRoute): { readonly text: string; readonly tone: "ok" | "wait" | "fault" } {
  const { status } = route;
  if (status.blockedSince !== null) return { text: "Blocked", tone: "fault" };
  if (status.active === null) return { text: "No request yet", tone: "wait" };
  const entry = status.entries.find((e) => e.connection === status.active);
  const troubled = entry !== undefined && (entry.exhausted.length > 0 || entry.problem !== null);
  return { text: entry?.name ?? status.active, tone: troubled ? "wait" : "ok" };
}

export function reasoningSentence(route: DeskRoute): string | null {
  switch (route.reasoning) {
    case "intact":
      return null;
    case "vendor-may-drop":
      return "This desk has changed connection. Its earlier hidden reasoning is still sent, and Anthropic leaves out whatever the new account or model cannot read. The visible conversation and the tool results are sent in full.";
    case "removed": {
      const active = route.status.entries.find((e) => e.connection === route.status.active);
      if (active !== undefined && kindTraits(active.kind).experimental) {
        return "This desk is on an experimental translated route. Hidden reasoning is left out of every request sent to OpenAI, and OpenAI's own hidden reasoning is not kept from one request to the next. The visible conversation and the tool results are sent in full.";
      }
      return "This desk has changed connection. Hidden reasoning is removed from every request it sends through OpenRouter. The visible conversation and the tool results are sent in full.";
    }
  }
}

export function switchSentence(route: RouteStatus): string | null {
  const move = route.lastSwitch;
  if (move === null) return null;
  const from = nameOf(route, move.from);
  const to = nameOf(route, move.to);
  return move.reason === "exhausted"
    ? `Moved from ${from} to ${to} at ${clock(move.at)}, because the allowance of ${from} was used up.`
    : `Switched from ${from} to ${to} at ${clock(move.at)}, by you.`;
}

export function servedSentence(route: RouteStatus): string | null {
  const served = route.lastServed;
  if (served === null) return null;
  const asked = `Last request asked for ${served.requestedModel}${served.requestedEffort === null ? "" : ` at ${served.requestedEffort} effort`}`;
  if (served.model === null && served.effort === served.requestedEffort) return `${asked} and was sent as asked.`;
  return `${asked} and was sent as ${served.model ?? served.requestedModel}${served.effort === null ? "" : ` at ${served.effort} effort`}.`;
}

/** One exhausted scope, with what happens next. `now` in Unix seconds. */
export function scopeSentence(scope: ExhaustedScope, now: number): string {
  const head = `${bucketLabel(scope.bucket)} is used up${scope.window === "five_hour" ? " (the five-hour limit)" : ""}, seen at ${clock(scope.observedAt)}.`;
  if (scope.checking) return `${head} A request is checking it now.`;
  if (scope.retryArmed) return `${head} A retry is armed: the next request that needs it checks it once.`;
  if (scope.resetAt === null) return `${head} The provider gave no reset time, so it stays shut until you retry.`;
  if (now < scope.resetAt) return `${head} The provider says it resets at ${clock(scope.resetAt)}. One check is allowed after that.`;
  return `${head} Its reset time has passed. One check is allowed, on the next request that needs it. If that check failed, press Retry.`;
}

export function entryStateSentence(entry: RouteEntry): string {
  switch (entry.state.kind) {
    case "active":
      return "Serving this desk";
    case "ready":
      return "Ready";
    case "unavailable":
      return `Not used: ${skipSentence(entry.kind, entry.state.reason)}`;
  }
}

export function canRetry(entry: RouteEntry): boolean {
  return entry.exhausted.some((s) => !s.checking && !s.retryArmed);
}

export function canSwitch(entry: RouteEntry): boolean {
  return entry.state.kind === "ready" || (entry.state.kind === "unavailable" && entry.state.reason.kind === "exhausted");
}
