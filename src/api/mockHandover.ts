import type { DeskHandover, HandoverSettings, HandoverView } from "../types";
import type { HandoverApi } from "./HandoverApi";

/** The handover group of a fresh install: off, nothing chosen (`HandoverSettings::default`). */
export function makeHandoverSettings(): HandoverSettings {
  return { automatic: false, claudeToCodex: null, codexToClaude: null };
}

/** The handover part of a snapshot: no used-up subscription, and the given desks. */
export function makeHandoverView(desks: readonly DeskHandover[] = []): HandoverView {
  return { usedUp: [], desks };
}

/** The browser-only stand-in for the handover command. Records the desks and ends nothing. */
export class MockHandover implements HandoverApi {
  /** Every desk handed over, in order. For tests. */
  readonly handed: string[] = [];
  private failure: string | null = null;

  /** Make the next call reject with `reason`. For tests. */
  failNext(reason: string): void {
    this.failure = reason;
  }

  async handOverDesk(sessionId: string): Promise<void> {
    if (this.failure !== null) {
      const reason = this.failure;
      this.failure = null;
      throw new Error(reason);
    }
    this.handed.push(sessionId);
  }
}
