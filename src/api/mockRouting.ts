import { kindTraits } from "../lib/routing";
import { skipSentence } from "../lib/route";
import type { DeskRoute, ExhaustedScope, FloorRouting, RouteEntry, RoutingKeyStatus, RoutingSettings, Settings } from "../types";
import type { RoutingApi } from "./RoutingApi";

/** The routing group of a fresh install: off, nothing seeded (`RoutingSettings::default`). */
export function makeRoutingSettings(): RoutingSettings {
  return { enabled: false, contexts: [], connections: [], tiers: { fable: [], opus: [], sonnet: [], haiku: [] } };
}

const KEY_MAX = 500;

/** A small valid route for tests and the demo: two connections, the first active. */
export function makeDeskRoute(overrides: Partial<DeskRoute> = {}): DeskRoute {
  return {
    status: {
      context: "work",
      generation: 1,
      active: "sub",
      blockedSince: null,
      lastSwitch: null,
      lastServed: null,
      entries: [
        { connection: "sub", name: "Claude login", kind: "claude-subscription", state: { kind: "active" }, exhausted: [], problem: null },
        { connection: "api", name: "Anthropic key", kind: "anthropic-api-key", state: { kind: "ready" }, exhausted: [], problem: null },
      ],
    },
    reasoning: "intact",
    requests: 0,
    recordsProblem: null,
    ...overrides,
  };
}

const armed = (scopes: readonly ExhaustedScope[]): ExhaustedScope[] => scopes.map((s) => (s.checking ? s : { ...s, retryArmed: true }));

/** The browser-only stand-in for the routing commands. Holds which connections have a key, never the key. */
export class MockRouting implements RoutingApi {
  private readonly withKey = new Set<string>();
  private readonly floorContext = new Map<string, string>();
  private readonly routes = new Map<string, DeskRoute>();

  /** For tests and the demo: the route of a desk, or none with null. */
  setRoute(sessionId: string, route: DeskRoute | null): void {
    if (route === null) this.routes.delete(sessionId);
    else this.routes.set(sessionId, route);
  }

  /** For tests: what is held, to show that no key text is among it. */
  held(): readonly string[] {
    return [...this.withKey, ...this.floorContext.keys(), ...this.floorContext.values()];
  }

  constructor(private readonly loadSettings: () => Promise<Settings>) {}

  async keyStatus(connectionId: string): Promise<RoutingKeyStatus> {
    await this.keyed(connectionId);
    return this.status(connectionId);
  }

  async setKey(connectionId: string, key: string | null): Promise<RoutingKeyStatus> {
    await this.keyed(connectionId);
    if (key === null) {
      this.withKey.delete(connectionId);
    } else {
      const text = key.trim();
      if (text.length === 0 || text.length > KEY_MAX || !/^[\x21-\x7e]+$/.test(text)) throw new Error("that does not look like an API key");
      this.withKey.add(connectionId);
    }
    return this.status(connectionId);
  }

  async floorRouting(projectId: string): Promise<FloorRouting> {
    return this.view(projectId);
  }

  async setFloorContext(projectId: string, contextId: string | null): Promise<FloorRouting> {
    const { routing } = await this.loadSettings();
    if (contextId === null) {
      this.floorContext.delete(projectId);
    } else {
      if (!routing.contexts.some((c) => c.id === contextId)) throw new Error("that account context is no longer in Settings");
      this.floorContext.set(projectId, contextId);
    }
    return this.view(projectId);
  }

  async deskRoute(sessionId: string): Promise<DeskRoute | null> {
    return this.routes.get(sessionId) ?? null;
  }

  async retryRoute(sessionId: string, connectionId: string): Promise<DeskRoute> {
    const { route, entry } = this.entryOf(sessionId, connectionId);
    if (entry.exhausted.length === 0) throw new Error("that connection has no used-up allowance on record, so there is nothing to retry");
    return this.replaceEntry(sessionId, route, { ...entry, exhausted: armed(entry.exhausted) });
  }

  async switchRoute(sessionId: string, connectionId: string): Promise<DeskRoute> {
    const { route, entry } = this.entryOf(sessionId, connectionId);
    if (entry.state.kind === "unavailable" && entry.state.reason.kind !== "exhausted") {
      throw new Error(`${entry.name} cannot be switched to, because ${skipSentence(entry.kind, entry.state.reason)}`);
    }
    const { status } = route;
    const entries = status.entries.map((e) => {
      if (e.connection === connectionId) return { ...e, state: { kind: "active" } as const, exhausted: armed(e.exhausted) };
      return e.connection === status.active ? { ...e, state: { kind: "ready" } as const } : e;
    });
    const lastSwitch = status.active === null ? status.lastSwitch : { from: status.active, to: connectionId, reason: "manual" as const, at: Math.floor(Date.now() / 1000) };
    const next = { ...route, status: { ...status, entries, active: connectionId, generation: status.generation + 1, blockedSince: null, lastSwitch } };
    this.routes.set(sessionId, next);
    return next;
  }

  private entryOf(sessionId: string, connectionId: string): { readonly route: DeskRoute; readonly entry: RouteEntry } {
    const route = this.routes.get(sessionId);
    if (route === undefined) throw new Error("that desk is not routed");
    const entry = route.status.entries.find((e) => e.connection === connectionId);
    if (entry === undefined) throw new Error("that connection is not in this desk's order");
    return { route, entry };
  }

  private replaceEntry(sessionId: string, route: DeskRoute, entry: RouteEntry): DeskRoute {
    const entries = route.status.entries.map((e) => (e.connection === entry.connection ? entry : e));
    const next = { ...route, status: { ...route.status, entries } };
    this.routes.set(sessionId, next);
    return next;
  }

  /** The connection, when it is in Settings and its kind takes a key. */
  private async keyed(connectionId: string): Promise<RoutingSettings["connections"][number]> {
    const connection = await this.connection(connectionId);
    if (!kindTraits(connection.kind).holdsKey) throw new Error("that kind of connection takes no API key");
    return connection;
  }

  private async connection(connectionId: string): Promise<RoutingSettings["connections"][number]> {
    const { routing } = await this.loadSettings();
    const found = routing.connections.find((c) => c.id === connectionId);
    if (found === undefined) throw new Error("that connection is no longer in Settings");
    return found;
  }

  private status(connectionId: string): RoutingKeyStatus {
    return { kind: this.withKey.has(connectionId) ? "saved" : "missing" };
  }

  private async view(projectId: string): Promise<FloorRouting> {
    const { routing } = await this.loadSettings();
    const stored = this.floorContext.get(projectId);
    const context = stored !== undefined && routing.contexts.some((c) => c.id === stored) ? stored : null;
    return { enabled: routing.enabled, context, contexts: routing.contexts.map(({ id, name }) => ({ id, name })) };
  }
}
