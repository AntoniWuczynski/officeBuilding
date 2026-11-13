import { z } from "zod";
import type { FloorRouting, RoutingKeyStatus, RoutingSettings } from "../types";

// The routing half of the wire schemas, kept apart from wire.ts. Same rule: `satisfies` proves they match types.ts.

const connectionKind = z.enum(["claude-subscription", "anthropic-api-key", "openrouter", "openai-api-key", "chatgpt-sign-in"]);
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
