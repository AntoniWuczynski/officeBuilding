import { useReducedMotion } from "framer-motion";
import type { Pet, PetKind } from "../../types";
import { DESK } from "../../lib/layout";
import type { Slot } from "../../lib/layout";
import { usePresence } from "../../state/usePresence";
import { Billboard } from "../../scene/Iso";
import { PetFigure } from "../../scene/Pet";

/** Past this many, the rest show as a count. */
const SHOWN = 4;
const GAP = 12;

const KIND_WORD: Record<PetKind, string> = {
  shell: "Shell",
  monitor: "Monitor",
  browser: "Browser",
  process: "Program",
};

/** What a session left running, on the floor in front of its desk: each pops in when it starts and out when it ends. */
export function Pets({ desk, z, pets }: { readonly desk: Slot; readonly z: number; readonly pets: readonly Pet[] }): React.ReactElement {
  const reduced = useReducedMotion() ?? false;
  const { list, done } = usePresence(pets.slice(0, SHOWN), (p) => p.id, !reduced);
  const y = desk.y + DESK.d + 7;
  return (
    <>
      {list.map(({ key, item, phase }, i) => (
        <Billboard key={key} x={desk.x + 4 + i * GAP} y={y} z={z}>
          <span
            className={`pet is-${phase}`}
            data-testid="pet"
            data-kind={item.kind}
            role="img"
            aria-label={`${KIND_WORD[item.kind]}: ${item.label}`}
            title={`${KIND_WORD[item.kind]}: ${item.label}`}
            onAnimationEnd={(e) => {
              if (phase === "leaving" && e.animationName === "pet-pop-out") done(key);
            }}
          >
            <PetFigure kind={item.kind} />
          </span>
        </Billboard>
      ))}
      {pets.length > SHOWN ? (
        <Billboard x={desk.x + 4 + SHOWN * GAP} y={y} z={z + 4}>
          <span className="pod-more" data-testid="pets-more">
            +{pets.length - SHOWN}
          </span>
        </Billboard>
      ) : null}
    </>
  );
}
