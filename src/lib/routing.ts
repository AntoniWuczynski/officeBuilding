import type { AccountContext, ConnectionKind, PerTier, RoutingConnection, RoutingSettings, TierId, TierTarget } from "../types";

// The pure half of the Routing group: every edit is a function from settings to new settings.
// The limits and the kind table mirror routing/config.rs.

export const NAME_MAX = 60;
export const CONTEXTS_MAX = 8;
export const CONNECTIONS_MAX = 24;

export const TIERS: readonly { readonly id: TierId; readonly label: string }[] = [
  { id: "fable", label: "Fable / Astra" },
  { id: "opus", label: "Opus / Sol" },
  { id: "sonnet", label: "Sonnet / Terra" },
  { id: "haiku", label: "Haiku / Luna" },
];

/** What a kind is called to the person. A new kind does not compile until it is named here. */
export const KIND_LABEL: Record<ConnectionKind, string> = {
  "claude-subscription": "Claude subscription",
  "anthropic-api-key": "Anthropic API key",
  openrouter: "OpenRouter",
  "openai-api-key": "OpenAI API key",
  "chatgpt-sign-in": "Sign in with ChatGPT",
};

const KIND_ORDER: readonly ConnectionKind[] = ["claude-subscription", "anthropic-api-key", "openrouter", "openai-api-key", "chatgpt-sign-in"];

export const KINDS: readonly { readonly kind: ConnectionKind; readonly label: string }[] = KIND_ORDER.map((kind) => ({ kind, label: KIND_LABEL[kind] }));

interface KindTraits {
  readonly metered: boolean;
  readonly experimental: boolean;
  readonly holdsKey: boolean;
  readonly passesThrough: boolean;
}

const TRAITS: Readonly<Record<ConnectionKind, KindTraits>> = {
  "claude-subscription": { metered: false, experimental: false, holdsKey: false, passesThrough: true },
  "anthropic-api-key": { metered: true, experimental: false, holdsKey: true, passesThrough: false },
  openrouter: { metered: true, experimental: false, holdsKey: true, passesThrough: false },
  "openai-api-key": { metered: true, experimental: true, holdsKey: true, passesThrough: false },
  "chatgpt-sign-in": { metered: false, experimental: true, holdsKey: false, passesThrough: false },
};

/** The same table as `ConnectionKind`'s methods in routing/config.rs. */
export function kindTraits(kind: ConnectionKind): KindTraits {
  return TRAITS[kind];
}

/** Whether the picker may offer `kind`, and if not, why (a sentence). */
export function kindOffer(settings: RoutingSettings, kind: ConnectionKind): string | null {
  if (kind === "chatgpt-sign-in") return "Sign in with ChatGPT is not built yet. OpenAI has to accept the building for ChatGPT plan use first.";
  if (settings.connections.length >= CONNECTIONS_MAX) return `There can be at most ${CONNECTIONS_MAX} connections.`;
  if (kind === "claude-subscription" && settings.connections.some((c) => c.kind === kind)) {
    return "The Claude login on this Mac can belong to one context only.";
  }
  return null;
}

/** A new id: `prefix` then 8 hex characters, not the id of a context or connection in `s`. `random` defaults to crypto.randomUUID. */
export function newId(prefix: "ctx" | "con", s: RoutingSettings, random: () => string = () => crypto.randomUUID()): string {
  const taken = new Set([...s.contexts.map((c) => c.id), ...s.connections.map((c) => c.id)]);
  for (;;) {
    const id = `${prefix}-${random().replaceAll("-", "").slice(0, 8)}`;
    if (!taken.has(id)) return id;
  }
}

/** The trimmed name, or null when it is empty or longer than NAME_MAX. */
export function cleanName(text: string): string | null {
  const name = text.trim();
  return name.length === 0 || [...name].length > NAME_MAX ? null : name;
}

export function addContext(s: RoutingSettings, id: string, name: string): RoutingSettings {
  return { ...s, contexts: [...s.contexts, { id, name, chain: [] }] };
}

export function renameContext(s: RoutingSettings, id: string, name: string): RoutingSettings {
  return { ...s, contexts: s.contexts.map((c) => (c.id === id ? { ...c, name } : c)) };
}

/** Also removes its connections. */
export function removeContext(s: RoutingSettings, id: string): RoutingSettings {
  return { ...s, contexts: s.contexts.filter((c) => c.id !== id), connections: s.connections.filter((c) => c.context !== id) };
}

const noTargets: PerTier<null> = { fable: null, opus: null, sonnet: null, haiku: null };

/** Appended to the end of the context's order, named after its kind, with no opt-in and no targets. */
export function addConnection(s: RoutingSettings, id: string, contextId: string, kind: ConnectionKind): RoutingSettings {
  const connection: RoutingConnection = { id, name: KIND_LABEL[kind], kind, context: contextId, paid: false, experimental: false, targets: noTargets };
  return {
    ...s,
    connections: [...s.connections, connection],
    contexts: s.contexts.map((c) => (c.id === contextId ? { ...c, chain: [...c.chain, id] } : c)),
  };
}

export function removeConnection(s: RoutingSettings, id: string): RoutingSettings {
  return {
    ...s,
    connections: s.connections.filter((c) => c.id !== id),
    contexts: s.contexts.map((c): AccountContext => ({ ...c, chain: c.chain.filter((x) => x !== id) })),
  };
}

/** Swap a connection with its neighbour in its context's order. Out of range: unchanged. */
export function moveConnection(s: RoutingSettings, id: string, by: -1 | 1): RoutingSettings {
  return {
    ...s,
    contexts: s.contexts.map((c) => {
      const from = c.chain.indexOf(id);
      const to = from + by;
      if (from < 0 || to < 0 || to >= c.chain.length) return c;
      const chain = [...c.chain];
      [chain[from], chain[to]] = [c.chain[to] ?? id, id];
      return { ...c, chain };
    }),
  };
}

export function patchConnection(s: RoutingSettings, id: string, change: Partial<Pick<RoutingConnection, "name" | "paid" | "experimental">>): RoutingSettings {
  return { ...s, connections: s.connections.map((c) => (c.id === id ? { ...c, ...change } : c)) };
}

/** An empty model text clears the tier's target. */
export function setTarget(s: RoutingSettings, id: string, tier: TierId, model: string, efforts: Readonly<Record<string, string>>): RoutingSettings {
  const name = model.trim();
  const target: TierTarget | null = name.length === 0 ? null : { model: name, efforts };
  return { ...s, connections: s.connections.map((c) => (c.id === id ? { ...c, targets: { ...c.targets, [tier]: target } } : c)) };
}

export function setTierModels(s: RoutingSettings, tier: TierId, models: readonly string[]): RoutingSettings {
  return { ...s, tiers: { ...s.tiers, [tier]: models } };
}

/** "xhigh=high, max=high" to a map. Null when a pair is not `name=name`. An empty text is an empty map. */
export function parseEfforts(text: string): Record<string, string> | null {
  const efforts: Record<string, string> = {};
  for (const pair of text.split(",")) {
    if (pair.trim().length === 0) continue;
    const [from, to, ...extra] = pair.split("=").map((part) => part.trim());
    if (from === undefined || to === undefined || extra.length > 0 || from.length === 0 || to.length === 0) return null;
    efforts[from] = to;
  }
  return efforts;
}

export function formatEfforts(efforts: Readonly<Record<string, string>>): string {
  return Object.entries(efforts)
    .map(([from, to]) => `${from}=${to}`)
    .join(", ");
}

/** Names separated by commas or white space, trimmed, empty ones dropped, duplicates dropped. */
export function parseModels(text: string): string[] {
  return [...new Set(text.split(/[\s,]+/).filter((name) => name.length > 0))];
}
