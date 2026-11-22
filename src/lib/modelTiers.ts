// Which tier worker draws a model, and which build its effort gets (GitHub issue #14).
// Data and two lookups: a model is matched on its family word, so a new version of a
// family lands on the same worker with no change here.

/** The effort builds, lean to buff. Each tier worker has one parts sheet for each (art/cast/raw/<worker>-<build>-sheet.png). */
export const BUILDS = ["minimal", "low", "medium", "high", "xhigh", "max"] as const;
export type Build = (typeof BUILDS)[number];

export const TIER_WORKERS = ["kit", "nell", "otto", "vera"] as const;
export type TierWorker = (typeof TIER_WORKERS)[number];

/** The model families of each tier (GitHub issue #14): Anthropic's first, OpenAI's second. */
export const TIER_FAMILIES: Readonly<Record<TierWorker, readonly [string, string]>> = {
  kit: ["haiku", "luna"],
  nell: ["sonnet", "terra"],
  otto: ["opus", "sol"],
  vera: ["fable", "astra"],
};

const FAMILY_TIER: ReadonlyMap<string, TierWorker> = new Map(TIER_WORKERS.flatMap((tier) => TIER_FAMILIES[tier].map((family) => [family, tier] as const)));

/**
 * The tier of a model id, or null for a model outside the table or a missing one. The family
 * must be a whole word of the id: Codex puts the version first (`gpt-6.2-sol`), and `solar`
 * is not `sol`.
 */
export function tierOf(model: string | null): TierWorker | null {
  for (const word of (model ?? "").toLowerCase().split(/[^a-z0-9]+/)) {
    const tier = FAMILY_TIER.get(word);
    if (tier !== undefined) return tier;
  }
  return null;
}

/** Effort ids by build. Tools name their levels alike but not all the same: Codex's `ultra` has no sheet of its own and takes the top build. */
export const EFFORT_BUILD: Readonly<Record<string, Build>> = {
  minimal: "minimal",
  low: "low",
  medium: "medium",
  high: "high",
  xhigh: "xhigh",
  max: "max",
  ultra: "max",
};

/** The build of an effort id, or null for an id outside the table (`default`, `thinking`) or a missing one. */
export function buildOf(effort: string | null): Build | null {
  const id = (effort ?? "").trim().toLowerCase();
  return Object.hasOwn(EFFORT_BUILD, id) ? (EFFORT_BUILD[id] ?? null) : null;
}

/** A build's name in words, as the Hire sheet labels the effort (`effort_label` in options.rs). */
export const BUILD_LABEL: Readonly<Record<Build, string>> = { minimal: "Minimal", low: "Low", medium: "Medium", high: "High", xhigh: "Extra high", max: "Max" };
