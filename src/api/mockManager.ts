import type { ManagerFloor, ManagerSettings } from "../types";
import { MANAGER_MAX_DESKS, MANAGER_SPEND_CEILING_USD } from "../lib/managerLimits";
import { between, inRange } from "../lib/qaLimits";

/** The Manager group of a fresh install (`ManagerSettings::default` in settings.rs). */
export function makeManagerSettings(): ManagerSettings {
  return { tool: "claude-code", model: null, effort: null, maxDesks: 3, spendCeilingUsd: 20 };
}

/** Why a Manager patch would be refused, with the backend's words, or null. */
export function managerPatchError(manager: ManagerSettings): string | null {
  if (!inRange(manager.maxDesks, MANAGER_MAX_DESKS)) return `desks at once must be ${between(MANAGER_MAX_DESKS)}`;
  if (!inRange(manager.spendCeilingUsd, MANAGER_SPEND_CEILING_USD)) return `the spend ceiling must be ${between(MANAGER_SPEND_CEILING_USD)} dollars`;
  return null;
}

/** A floor with nobody hired and the caps the settings give. */
export function makeAbsentManager(projectId: string): ManagerFloor {
  const { maxDesks, spendCeilingUsd } = makeManagerSettings();
  return {
    projectId,
    status: { kind: "absent" },
    paused: false,
    mayHire: false,
    escalated: false,
    managed: [],
    caps: { maxDesks, hiredNow: 0, spendCeilingUsd, spentUsd: 0, unpricedDesks: 0 },
    queue: [],
    log: [],
  };
}

/** The demo managers, one per seeded floor (invented data). */
export function makeSeedManagers(): ManagerFloor[] {
  const shop: ManagerFloor = {
    ...makeAbsentManager("p-shop"),
    status: { kind: "hired", sessionId: null, startedAt: "2026-09-28T15:00:00Z", lastCallAt: "2026-09-28T15:37:00Z" },
    mayHire: true,
    managed: ["s-1"],
    caps: { maxDesks: 3, hiredNow: 1, spendCeilingUsd: 20, spentUsd: 1.82, unpricedDesks: 0 },
    queue: [{ id: "n-2", toSessionId: "s-1", text: "When the checkout audit is done, list the files you changed.", queuedAt: "2026-09-28T15:36:00Z", hold: "busy" }],
    log: [
      { id: "l-3", at: "2026-09-28T15:37:00Z", event: { kind: "refused", tool: "hire_desk", reason: "3 desks are already hired" } },
      { id: "l-2", at: "2026-09-28T15:36:00Z", event: { kind: "note", noteId: "n-2", toSessionId: "s-1", toTitle: "audit checkout", text: "When the checkout audit is done, list the files you changed.", state: "queued" } },
      { id: "l-1", at: "2026-09-28T15:20:00Z", event: { kind: "note", noteId: "n-1", toSessionId: "s-1", toTitle: "audit checkout", text: "Start with the price list.", state: "delivered" } },
    ],
  };
  return [shop, makeAbsentManager("p-office"), { ...makeAbsentManager("p-arcade"), status: { kind: "hired", sessionId: null, startedAt: "2026-09-28T14:00:00Z", lastCallAt: null }, paused: true }, makeAbsentManager("p-notes")];
}

/** The list with a floor's manager hired. A floor with no entry gets one. */
export function withManagerHired(list: readonly ManagerFloor[], projectId: string, startedAt: string): ManagerFloor[] {
  const hire = (m: ManagerFloor): ManagerFloor => ({ ...m, status: { kind: "hired", sessionId: null, startedAt, lastCallAt: null } });
  if (!list.some((m) => m.projectId === projectId)) return [...list, hire(makeAbsentManager(projectId))];
  return list.map((m) => (m.projectId === projectId ? hire(m) : m));
}

/** The list with one of a floor's switches set. A floor with no entry gets one. */
export function withManagerSwitch(list: readonly ManagerFloor[], projectId: string, key: "paused" | "mayHire", on: boolean): ManagerFloor[] {
  if (!list.some((m) => m.projectId === projectId)) return [...list, { ...makeAbsentManager(projectId), [key]: on }];
  return list.map((m) => (m.projectId === projectId ? { ...m, [key]: on } : m));
}

/** Whether a floor has this note waiting. */
export function hasQueuedNote(list: readonly ManagerFloor[], projectId: string, noteId: string): boolean {
  return list.some((m) => m.projectId === projectId && m.queue.some((n) => n.id === noteId));
}

/** The list with a waiting note taken out of its floor's queue and marked withdrawn in the log. */
export function withNoteWithdrawn(list: readonly ManagerFloor[], projectId: string, noteId: string): ManagerFloor[] {
  return list.map((m) => {
    if (m.projectId !== projectId) return m;
    return {
      ...m,
      queue: m.queue.filter((n) => n.id !== noteId),
      log: m.log.map((e) => (e.event.kind === "note" && e.event.noteId === noteId ? { ...e, event: { ...e.event, state: "withdrawn" as const } } : e)),
    };
  });
}
