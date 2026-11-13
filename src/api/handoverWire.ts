import { z } from "zod";
import type { BackgroundTask, DeskHandover, DeskHandoverState, DeskWork, HandoverSettings, HandoverView, OfferWhy, StartWith, UsedUpView } from "../types";

// The handover half of the wire schemas, kept apart from wire.ts. Same rule: `satisfies` proves they match types.ts.

const handoverTool = z.enum(["claude-code", "codex"]);
const limitWindow = z.enum(["five-hour", "seven-day"]);
const time = z.number();

const backgroundTask = z.object({
  kind: z.enum(["workflow", "subagent", "shell", "scheduled", "goal", "other"]),
  name: z.string().nullable(),
}) satisfies z.ZodType<BackgroundTask>;

const deskWork = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("clear") }),
  z.object({ kind: z.literal("running"), tasks: z.array(backgroundTask) }),
  z.object({ kind: z.literal("unknown") }),
]) satisfies z.ZodType<DeskWork>;

const offerWhy = z.discriminatedUnion("kind", [
  z.object({ kind: z.enum(["automatic-off", "work-running", "work-unknown", "seen-elsewhere", "not-at-the-time"]) }),
  z.object({ kind: z.literal("failed"), reason: z.string() }),
]) satisfies z.ZodType<OfferWhy>;

const deskState = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("offered"),
    to: handoverTool,
    why: offerWhy,
    work: deskWork,
    window: limitWindow.nullable(),
    resetAt: time.nullable(),
  }),
  z.object({
    kind: z.literal("blocked"),
    to: handoverTool,
    why: z.enum(["not-chosen", "both-used-up", "already-handed-over"]),
    window: limitWindow.nullable(),
    resetAt: time.nullable(),
    otherResetAt: time.nullable(),
  }),
  z.object({ kind: z.literal("underway"), to: handoverTool }),
  z.object({ kind: z.literal("arrived"), from: handoverTool, at: time, window: limitWindow.nullable(), resetAt: time.nullable() }),
  z.object({ kind: z.literal("left"), to: handoverTool, at: time }),
  z.object({ kind: z.literal("failed"), to: handoverTool, at: time, reason: z.string() }),
]) satisfies z.ZodType<DeskHandoverState>;

const deskHandover = z.object({ sessionId: z.string(), state: deskState }) satisfies z.ZodType<DeskHandover>;

const usedUp = z.object({
  tool: handoverTool,
  window: limitWindow.nullable(),
  resetAt: time.nullable(),
  seenAt: time,
}) satisfies z.ZodType<UsedUpView>;

export const handoverViewSchema = z.object({
  usedUp: z.array(usedUp),
  desks: z.array(deskHandover),
}) satisfies z.ZodType<HandoverView>;

const startWith = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("tool-default") }),
  z.object({ kind: z.literal("chosen"), model: z.string(), effort: z.string() }),
]) satisfies z.ZodType<StartWith>;

export const handoverSettingsSchema = z.object({
  automatic: z.boolean(),
  claudeToCodex: startWith.nullable(),
  codexToClaude: startWith.nullable(),
}) satisfies z.ZodType<HandoverSettings>;
