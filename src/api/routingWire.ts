import { z } from "zod";
import type { DeskRoute, ExhaustedScope, FloorRouting, RouteBucket, RouteEntry, RouteEntryState, RouteSkip, RouteStatus, RoutingKeyStatus, RoutingSettings } from "../types";

// The routing half of the wire schemas, kept apart from wire.ts. Same rule: `satisfies` proves they match types.ts.

const connectionKind = z.enum(["claude-subscription", "anthropic-api-key", "openrouter", "openai-api-key", "chatgpt-sign-in"]);
const tierId = z.enum(["fable", "opus", "sonnet", "haiku"]);
const perTier = <T extends z.ZodType>(value: T) => z.object({ fable: value, opus: value, sonnet: value, haiku: value });
const tierTarget = z.object({ model: z.string(), efforts: z.record(z.string(), z.string()) });

export const routingSettingsSchema = z.object({
  enabled: z.boolean(),
  contexts: z.array(z.object({ id: z.string(), name: z.string(), chain: z.array(z.string()) })),
  connections: z.array(
    z.object({
      id: z.string(),
      name: z.string(),
      kind: connectionKind,
      context: z.string(),
      paid: z.boolean(),
      experimental: z.boolean(),
      targets: perTier(tierTarget.nullable()),
    }),
  ),
  tiers: perTier(z.array(z.string())),
}) satisfies z.ZodType<RoutingSettings>;

export const routingKeyStatusSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("missing") }),
  z.object({ kind: z.literal("saved") }),
  z.object({ kind: z.literal("unavailable"), reason: z.string() }),
]) satisfies z.ZodType<RoutingKeyStatus>;

export const floorRoutingSchema = z.object({
  enabled: z.boolean(),
  context: z.string().nullable(),
  contexts: z.array(z.object({ id: z.string(), name: z.string() })),
}) satisfies z.ZodType<FloorRouting>;

const routeSkip = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("removed") }),
  z.object({ kind: z.literal("paid-not-allowed") }),
  z.object({ kind: z.literal("experimental-not-allowed") }),
  z.object({ kind: z.literal("no-credential") }),
  z.object({ kind: z.literal("unmapped-model"), model: z.string() }),
  z.object({ kind: z.literal("no-target"), tier: tierId }),
  z.object({ kind: z.literal("effort-unmapped"), tier: tierId, effort: z.string() }),
  z.object({ kind: z.literal("unsupported"), capability: z.enum(["tools", "images", "documents", "server-tools"]) }),
  z.object({ kind: z.literal("exhausted") }),
  z.object({ kind: z.literal("tried") }),
]) satisfies z.ZodType<RouteSkip>;

const routeBucket = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("whole") }),
  z.object({ kind: z.literal("tier"), tier: tierId }),
  z.object({ kind: z.literal("model"), model: z.string() }),
]) satisfies z.ZodType<RouteBucket>;

const exhaustedScope = z.object({
  bucket: routeBucket,
  window: z.string().nullable(),
  observedAt: z.number(),
  resetAt: z.number().nullable(),
  checking: z.boolean(),
  retryArmed: z.boolean(),
}) satisfies z.ZodType<ExhaustedScope>;

const routeEntryState = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("active") }),
  z.object({ kind: z.literal("ready") }),
  z.object({ kind: z.literal("unavailable"), reason: routeSkip }),
]) satisfies z.ZodType<RouteEntryState>;

const routeEntry = z.object({
  connection: z.string(),
  name: z.string(),
  kind: connectionKind,
  state: routeEntryState,
  exhausted: z.array(exhaustedScope),
  problem: z
    .object({ kind: z.enum(["throttled", "faulted", "unauthorised", "forbidden", "rejected", "unreachable", "interrupted"]), at: z.number() })
    .nullable(),
}) satisfies z.ZodType<RouteEntry>;

const routeStatus = z.object({
  context: z.string(),
  generation: z.number(),
  active: z.string().nullable(),
  blockedSince: z.number().nullable(),
  lastSwitch: z.object({ from: z.string(), to: z.string(), reason: z.enum(["exhausted", "manual"]), at: z.number() }).nullable(),
  lastServed: z
    .object({
      connection: z.string(),
      requestedModel: z.string(),
      requestedEffort: z.string().nullable(),
      model: z.string().nullable(),
      effort: z.string().nullable(),
      at: z.number(),
    })
    .nullable(),
  entries: z.array(routeEntry),
}) satisfies z.ZodType<RouteStatus>;

export const deskRouteSchema = z.object({
  status: routeStatus,
  reasoning: z.enum(["intact", "vendor-may-drop", "removed"]),
  requests: z.number(),
  recordsProblem: z.string().nullable(),
}) satisfies z.ZodType<DeskRoute>;
