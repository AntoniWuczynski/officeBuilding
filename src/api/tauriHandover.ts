import type { TakeOverStart } from "../types";
import type { HandoverApi } from "./HandoverApi";
import { call } from "./tauriApi";

/** The handover calls through the Tauri command bridge. */
export class TauriHandover implements HandoverApi {
  async handOverDesk(sessionId: string): Promise<void> {
    await call("hand_over_desk", { sessionId });
  }

  async takeOver(sessionId: string, start: TakeOverStart): Promise<void> {
    await call("take_over_desk", { sessionId, start });
  }
}
