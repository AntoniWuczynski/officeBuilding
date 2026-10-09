import type { PetKind } from "../types";
import { PETS } from "./cast/parts";
import type { StyleWithVars } from "./Iso";

type PetPart = keyof typeof PETS;

/** A piece hinged at `pin` (shares of its own size) to `at` (shares of the body), or the body itself. */
interface Piece {
  readonly part: PetPart;
  readonly pin: readonly [number, number];
  readonly at: readonly [number, number];
  readonly className: string;
}

/** Which animal stands for what an agent left running, body first, then pieces back to front. */
const ANIMALS: Record<PetKind, { readonly body: PetPart; readonly pieces: readonly Piece[] }> = {
  // A shell is a seashell.
  shell: { body: "shell", pieces: [] },
  // Claude Code's Monitor tool is an owl, head swivelling as it watches.
  monitor: { body: "owl-body", pieces: [{ part: "owl-head", pin: [0.5, 0.86], at: [0.5, 0.14], className: "pet-head" }] },
  // A browser the agent drives is a cat.
  browser: {
    body: "cat-body",
    pieces: [
      { part: "cat-tail", pin: [0.3, 0.95], at: [0.86, 0.9], className: "pet-tail pet-behind" },
      { part: "cat-head", pin: [0.5, 0.86], at: [0.5, 0.1], className: "pet-head" },
    ],
  },
  // Any other program it started is a dog.
  process: {
    body: "dog-body",
    pieces: [
      { part: "dog-tail", pin: [0.5, 0.95], at: [0.8, 0.82], className: "pet-tail pet-behind" },
      { part: "dog-head", pin: [0.5, 0.8], at: [0.5, 0.12], className: "pet-head" },
    ],
  },
};

/** A cut-out animal (or seashell) for something an agent left running, standing at its anchor. */
export function PetFigure({ kind }: { readonly kind: PetKind }): React.ReactElement {
  const animal = ANIMALS[kind];
  const body = PETS[animal.body];
  const bodyStyle: StyleWithVars = { left: -body.w / 2, top: -body.h, width: body.w, height: body.h };
  return (
    <span className={`pet-figure pet-${kind}`} aria-hidden>
      <span className="pet-rig">
        <span className="pp pet-body" style={bodyStyle}>
          {animal.pieces.map((p) => {
            const part = PETS[p.part];
            const style: StyleWithVars = {
              left: body.w * p.at[0] - part.w * p.pin[0],
              top: body.h * p.at[1] - part.h * p.pin[1],
              width: part.w,
              height: part.h,
              transformOrigin: `${part.w * p.pin[0]}px ${part.h * p.pin[1]}px`,
            };
            return (
              <span key={p.part} className={`pp ${p.className}`} style={style}>
                <img src={part.src} alt="" draggable={false} />
              </span>
            );
          })}
          <img className="pet-body-img" src={body.src} alt="" draggable={false} />
        </span>
      </span>
    </span>
  );
}
