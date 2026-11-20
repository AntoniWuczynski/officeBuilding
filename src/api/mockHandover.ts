import type { DeskHandover, HandoverSettings, HandoverView, TakeOverStart } from "../types";
import type { HandoverApi } from "./HandoverApi";

/** The handover group of a fresh install: off, nothing chosen (`HandoverSettings::default`). */
export function makeHandoverSettings(): HandoverSettings {
  return { automatic: false, claudeToCodex: null, codexToClaude: null };
}

/** The handover part of a snapshot: no used-up subscription, and the given desks. */
export function makeHandoverView(desks: readonly DeskHandover[] = []): HandoverView {
  return { usedUp: [], desks, takeOvers: [] };
}

/** The browser-only stand-in for the handover command. Records the desks and ends nothing. */
export class MockHandover implements HandoverApi {
  /** Every desk handed over, in order. For tests. */
  readonly handed: string[] = [];
  /** Every take-over asked for, in order. For tests. */
  readonly taken: { readonly sessionId: string; readonly start: TakeOverStart }[] = [];
  private failure: string | null = null;

  /** Make the next call reject with `reason`. For tests. */
  failNext(reason: string): void {
    this.failure = reason;
  }

  private check(): void {
    if (this.failure === null) return;
    const reason = this.failure;
    this.failure = null;
    throw new Error(reason);
  }

  async handOverDesk(sessionId: string): Promise<void> {
    this.check();
    this.handed.push(sessionId);
  }

  async takeOver(sessionId: string, start: TakeOverStart): Promise<void> {
    this.check();
    this.taken.push({ sessionId, start });
  }
}
