import type { BackgroundTask, BackgroundTaskKind, BlockWhy, DeskHandoverState, DeskWork, HandoverTool, LimitWindow, OfferWhy } from "../types";
import { plural, TOOL_LABEL } from "../state/selectors";
import { clock } from "./route";

// Every sentence the handover shows. Pure, so the words are tested in one place.

type Offered = Extract<DeskHandoverState, { kind: "offered" }>;
type Blocked = Extract<DeskHandoverState, { kind: "blocked" }>;

export const SUBSCRIPTION: Record<HandoverTool, string> = { "claude-code": "Claude subscription", codex: "ChatGPT plan" };

const DAY_MS = 24 * 60 * 60 * 1000;
const WEEKDAY = new Intl.DateTimeFormat("en-GB", { weekday: "long" });

export function otherTool(tool: HandoverTool): HandoverTool {
  return tool === "claude-code" ? "codex" : "claude-code";
}

export function windowWords(window: LimitWindow | null): string {
  if (window === "five-hour") return "5-hour limit";
  if (window === "seven-day") return "weekly limit";
  return "limit";
}

export function resetSentence(window: LimitWindow | null, resetAt: number | null, now: Date): string {
  if (resetAt === null) return "The reset time is not known.";
  const ms = resetAt * 1000;
  const words = windowWords(window);
  if (ms <= now.getTime()) return `The ${words} should have reset by now.`;
  if (ms - now.getTime() < DAY_MS) return `The ${words} resets at ${clock(resetAt)}.`;
  return `The ${words} resets on ${WEEKDAY.format(ms)} at ${clock(resetAt)}.`;
}

function usedUpHead(tool: HandoverTool): string {
  return `The ${SUBSCRIPTION[tool]} is used up.`;
}

export function usedUpSentence(tool: HandoverTool, window: LimitWindow | null, resetAt: number | null, now: Date): string {
  return `${usedUpHead(tool)} ${resetSentence(window, resetAt, now)}`;
}

export function offerWhySentence(why: OfferWhy): string {
  switch (why.kind) {
    case "automatic-off":
      return "Automatic handover is off, so it waits for you.";
    case "work-running":
      return "It has background work running, so it was not handed over by itself.";
    case "work-unknown":
      return "The building cannot tell whether it has a workflow or other background work running, so it was not handed over by itself.";
    case "seen-elsewhere":
      return "Another desk on the same login reached the limit. This one has not been refused yet.";
    case "not-at-the-time":
      return "It was not handed over when it reached the limit, so it waits for you.";
    case "failed":
      return `The last handover did not work: ${why.reason}.`;
  }
}

export function blockSentence(state: Blocked): string {
  const to = TOOL_LABEL[state.to];
  const why: BlockWhy = state.why;
  switch (why) {
    case "not-chosen":
      return `To hand this desk over, choose what ${to} starts with in Settings, under Handover.`;
    case "both-used-up":
      return state.otherResetAt === null ? `${to} is used up too, so this desk stays.` : `${to} is used up too, until ${clock(state.otherResetAt)}, so this desk stays.`;
    case "already-handed-over":
      return `This desk was already handed over to ${to} for this limit.`;
  }
}

const KIND_WORDS: readonly { readonly kind: BackgroundTaskKind; readonly one: string; readonly many: string }[] = [
  { kind: "workflow", one: "workflow", many: "workflows" },
  { kind: "subagent", one: "background subagent", many: "background subagents" },
  { kind: "shell", one: "background shell", many: "background shells" },
  { kind: "scheduled", one: "scheduled wake-up", many: "scheduled wake-ups" },
  { kind: "other", one: "other background task", many: "other background tasks" },
];

function joinWords(parts: readonly string[]): string {
  const last = parts[parts.length - 1];
  if (last === undefined) return "";
  return parts.length === 1 ? last : `${parts.slice(0, -1).join(", ")} and ${last}`;
}

function taskWords(tasks: readonly BackgroundTask[]): string {
  const named = tasks.flatMap((t) => (t.kind === "workflow" && t.name !== null ? [`the workflow “${t.name}”`] : []));
  const counted = KIND_WORDS.flatMap(({ kind, one, many }) => {
    const n = tasks.filter((t) => t.kind === kind && !(kind === "workflow" && t.name !== null)).length;
    return n === 0 ? [] : [plural(n, one, many)];
  });
  return joinWords([...named, ...counted]);
}

export function endsSentence(work: DeskWork, from: HandoverTool): string {
  const head = `This ends the ${TOOL_LABEL[from]} session at this desk`;
  if (work.kind === "clear") return `${head}.`;
  if (work.kind === "running" && work.tasks.length > 0) return `${head}, and with it ${taskWords(work.tasks)}.`;
  return `${head}, and with it any workflow or background work it is running.`;
}

export function confirmSentence(state: Offered): string {
  const to = TOOL_LABEL[state.to];
  return `${endsSentence(state.work, otherTool(state.to))} Its conversation stays on disk and can be resumed from the sign-out sheet. ${to} starts in the same folder and is told to read a brief: the visible conversation with tool output cut short, and the state of the folder.`;
}

export function stripLine(state: DeskHandoverState, now: Date): { readonly tone: "ok" | "wait" | "fault"; readonly text: string } {
  switch (state.kind) {
    case "offered":
      return { tone: "wait", text: `${usedUpSentence(otherTool(state.to), state.window, state.resetAt, now)} ${offerWhySentence(state.why)}` };
    case "blocked":
      return { tone: "fault", text: `${usedUpSentence(otherTool(state.to), state.window, state.resetAt, now)} ${blockSentence(state)}` };
    case "underway":
      return { tone: "wait", text: `Handing this desk over to ${TOOL_LABEL[state.to]}.` };
    case "arrived":
      return {
        tone: "ok",
        text: `Handed over from ${TOOL_LABEL[state.from]} at ${clock(state.at)}, when the ${SUBSCRIPTION[state.from]} was used up. ${resetSentence(state.window, state.resetAt, now)}`,
      };
    case "left":
      return { tone: "ok", text: `Handed over to ${TOOL_LABEL[state.to]} at ${clock(state.at)}.` };
    case "failed":
      return { tone: "fault", text: `Handover to ${TOOL_LABEL[state.to]} failed at ${clock(state.at)}: ${state.reason}.` };
  }
}

export function tagWord(state: DeskHandoverState): string | null {
  switch (state.kind) {
    case "offered":
    case "blocked":
      return "used up";
    case "underway":
      return "handing over";
    case "arrived":
      return `from ${TOOL_LABEL[state.from]}`;
    case "left":
    case "failed":
      return null;
  }
}

export function cardLine(state: DeskHandoverState, _now: Date): string | null {
  switch (state.kind) {
    case "offered":
    case "blocked":
      return usedUpHead(otherTool(state.to));
    case "underway":
      return `Being handed over to ${TOOL_LABEL[state.to]}`;
    case "arrived":
      return `Handed over from ${TOOL_LABEL[state.from]} at ${clock(state.at)}`;
    case "left":
    case "failed":
      return null;
  }
}

export function signOutLine(state: DeskHandoverState): string | null {
  switch (state.kind) {
    case "left":
      return `Handed over to ${TOOL_LABEL[state.to]} at ${clock(state.at)}`;
    case "failed":
      return `Handover to ${TOOL_LABEL[state.to]} failed at ${clock(state.at)}: ${state.reason}`;
    case "offered":
    case "blocked":
    case "underway":
    case "arrived":
      return null;
  }
}
