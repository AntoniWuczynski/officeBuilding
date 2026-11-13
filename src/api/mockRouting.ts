import { kindTraits } from "../lib/routing";
import type { FloorRouting, RoutingKeyStatus, RoutingSettings, Settings } from "../types";
import type { RoutingApi } from "./RoutingApi";

/** The routing group of a fresh install: off, nothing seeded (`RoutingSettings::default`). */
export function makeRoutingSettings(): RoutingSettings {
  return { enabled: false, contexts: [], connections: [], tiers: { fable: [], opus: [], sonnet: [], haiku: [] } };
}

const KEY_MAX = 500;

/** The browser-only stand-in for the routing commands. Holds which connections have a key, never the key. */
export class MockRouting implements RoutingApi {
  private readonly withKey = new Set<string>();
  private readonly floorContext = new Map<string, string>();

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
