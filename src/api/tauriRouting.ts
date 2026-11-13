import type { DeskRoute, FloorRouting, RoutingKeyStatus } from "../types";
import type { RoutingApi } from "./RoutingApi";
import { deskRouteSchema, floorRoutingSchema, routingKeyStatusSchema } from "./routingWire";
import { call } from "./tauriApi";
import { parseWire } from "./wire";

/** The routing calls through the Tauri command bridge. */
export class TauriRouting implements RoutingApi {
  async keyStatus(connectionId: string): Promise<RoutingKeyStatus> {
    return parseWire(routingKeyStatusSchema, "key status", await call("routing_key_status", { connectionId }));
  }

  async setKey(connectionId: string, key: string | null): Promise<RoutingKeyStatus> {
    return parseWire(routingKeyStatusSchema, "key status", await call("set_routing_key", { connectionId, key }));
  }

  async floorRouting(projectId: string): Promise<FloorRouting> {
    return parseWire(floorRoutingSchema, "floor routing", await call("floor_routing", { projectId }));
  }

  async setFloorContext(projectId: string, contextId: string | null): Promise<FloorRouting> {
    return parseWire(floorRoutingSchema, "floor routing", await call("set_floor_context", { projectId, contextId }));
  }

  async deskRoute(sessionId: string): Promise<DeskRoute | null> {
    return parseWire(deskRouteSchema.nullable(), "desk route", await call("desk_route", { sessionId }));
  }

  async retryRoute(sessionId: string, connectionId: string): Promise<DeskRoute> {
    return parseWire(deskRouteSchema, "desk route", await call("retry_route", { sessionId, connectionId }));
  }

  async switchRoute(sessionId: string, connectionId: string): Promise<DeskRoute> {
    return parseWire(deskRouteSchema, "desk route", await call("switch_route", { sessionId, connectionId }));
  }
}
