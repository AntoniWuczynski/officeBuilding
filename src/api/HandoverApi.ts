import type { TakeOverStart } from "../types";

/** The handover calls of the backend (src-tauri/src/handover/commands.rs). */
export interface HandoverApi {
  /** Hand a used-up desk over to the other tool. Ends its session. Rejects with the reason in words. */
  handOverDesk(sessionId: string): Promise<void>;
  /** Start a new session that takes over from this one. Nothing is ended. Rejects with the reason in words. */
  takeOver(sessionId: string, start: TakeOverStart): Promise<void>;
}
