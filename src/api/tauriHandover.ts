import type { HandoverApi } from "./HandoverApi";
import { call } from "./tauriApi";

/** The handover call through the Tauri command bridge. */
export class TauriHandover implements HandoverApi {
  async handOverDesk(sessionId: string): Promise<void> {
    await call("hand_over_desk", { sessionId });
  }
}
