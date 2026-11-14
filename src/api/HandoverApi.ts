/** The handover call of the backend (src-tauri/src/handover/commands.rs). */
export interface HandoverApi {
  /** Hand a used-up desk over to the other tool. Ends its session. Rejects with the reason in words. */
  handOverDesk(sessionId: string): Promise<void>;
}
