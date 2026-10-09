// Each employee is a stable cast member derived from their session id, so the
// same agent is recognisable on the building and inside the floor.

import { AYO, BOSS, DEV, FERN, MARA, THEO, WALT } from "../scene/cast/parts";
import type { PersonParts } from "../scene/cast/rig";

export interface Look {
  /** Which cast member: the cut-out sheet it comes from. */
  readonly name: string;
  readonly parts: PersonParts;
}

const CAST: readonly [Look, ...Look[]] = [
  { name: "theo", parts: THEO },
  { name: "mara", parts: MARA },
  { name: "walt", parts: WALT },
  { name: "ayo", parts: AYO },
  { name: "dev", parts: DEV },
  { name: "fern", parts: FERN },
];

/** The manager's look: fixed, so the corner office is always the same person. */
export const MANAGER_LOOK: Look = { name: "boss", parts: BOSS };

/** FNV-1a, 32-bit. */
export function hashString(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export function lookFor(id: string): Look {
  return CAST[hashString(id) % CAST.length] ?? CAST[0];
}
