// Each employee is a stable cast member derived from their session id, so the
// same agent is recognisable on the building and inside the floor.

import { AYO, BOSS, DEV, FERN, MARA, THEO, TIER_BUILDS, WALT } from "../scene/cast/parts";
import type { PersonParts } from "../scene/cast/rig";
import type { Helper, ToolKind } from "../types";
import { buildOf, tierOf } from "./modelTiers";
import type { Build, TierWorker } from "./modelTiers";

export interface Look {
  /** Which cast member, and for a tier worker its build: "theo", "boss", "nell-high". */
  readonly name: string;
  readonly parts: PersonParts;
  /** The piece set whose sizes place the joints (rig.ts). Omitted: the parts themselves. */
  readonly frame?: PersonParts;
  /** A slight purple cast: a Claude Code desk hired with ultracode on. */
  readonly tint?: boolean;
}

/** Who is drawn: a session, an ended session, or a helper seen through `helperDrawn`. */
export interface Drawn {
  readonly id: string;
  readonly tool: ToolKind;
  readonly model: string | null;
  readonly effort: string | null;
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

const perBuild = <V>(f: (build: Build) => V): Readonly<Record<Build, V>> => ({ minimal: f("minimal"), low: f("low"), medium: f("medium"), high: f("high"), xhigh: f("xhigh"), max: f("max") });
const perTier = <V>(f: (worker: TierWorker) => V): Readonly<Record<TierWorker, V>> => ({ kit: f("kit"), nell: f("nell"), otto: f("otto"), vera: f("vera") });

/** The 24 tier looks: the medium sheet's head, closed-eyes head and chair on every build, which is also every build's frame. */
const TIER_LOOKS = perTier((worker) =>
  perBuild((build): Look => ({ name: `${worker}-${build}`, parts: { ...TIER_BUILDS[worker].medium, ...TIER_BUILDS[worker][build] }, frame: TIER_BUILDS[worker].medium })),
);

/** The look of a worker: its model's tier worker in its effort's build (medium when the effort says nothing), or the id's own cast member. */
export function lookOf(worker: Pick<Drawn, "id" | "model" | "effort">, ultracode?: boolean): Look {
  const tier = tierOf(worker.model);
  const look = tier === null ? lookFor(worker.id) : TIER_LOOKS[tier][buildOf(worker.effort) ?? "medium"];
  return ultracode === true ? { ...look, tint: true } : look;
}

/** A helper as it is drawn: its own model and effort, each falling back to its agent's, in its agent's tool. */
export function helperDrawn(agent: Drawn, helper: Helper): Drawn {
  return { id: helper.id, tool: agent.tool, model: helper.model ?? agent.model, effort: helper.effort ?? agent.effort };
}

/** Whether a worker carries the tint: a Claude Code session in the snapshot's `ultracode` list. */
export function isUltracode(ultracode: ReadonlySet<string>, worker: Pick<Drawn, "id" | "tool">): boolean {
  return worker.tool === "claude-code" && ultracode.has(worker.id);
}
