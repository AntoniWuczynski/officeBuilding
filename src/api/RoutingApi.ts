import type { FloorRouting, RoutingKeyStatus } from "../types";

/** The routing calls of the backend (src-tauri/src/routing/commands.rs). Stage B adds the live route calls here. */
export interface RoutingApi {
  /** What the building knows about a connection's API key. May read the Keychain. Never returns the key. */
  keyStatus(connectionId: string): Promise<RoutingKeyStatus>;
  /** Save a connection's API key or, with null, remove it. The key is not checked with the vendor. */
  setKey(connectionId: string, key: string | null): Promise<RoutingKeyStatus>;
  /** A floor's account context and the contexts it can pick from. */
  floorRouting(projectId: string): Promise<FloorRouting>;
  /** Give a floor an account context, or none with null. Resolves to the new view. */
  setFloorContext(projectId: string, contextId: string | null): Promise<FloorRouting>;
}
