// The one place a worker's look is decided: its model's tier, its effort's build and its
// ultracode tint (GitHub issue #14). The manager is not drawn here: it keeps MANAGER_LOOK.

import { useContext } from "react";
import type { Pose } from "../lib/animation";
import { isUltracode, lookOf } from "../lib/looks";
import type { Drawn, Look } from "../lib/looks";
import { UltracodeContext } from "../state/UltracodeContext";
import { Person, Walker } from "./Person";
import type { Handed } from "./Person";

function useLook(of: Drawn): Look {
  return lookOf(of, isUltracode(useContext(UltracodeContext), of));
}

/** A seated worker (or flat on the floor when stuck), in the look of its model, effort and ultracode. */
export function Worker({ of, pose, receiving = null }: { readonly of: Drawn; readonly pose: Pose; readonly receiving?: Handed | null }): React.ReactElement {
  return <Person look={useLook(of)} pose={pose} receiving={receiving} />;
}

/** The same worker, walking. */
export function WalkingWorker({ of, carrying }: { readonly of: Drawn; readonly carrying: Handed | null }): React.ReactElement {
  return <Walker look={useLook(of)} carrying={carrying} />;
}

/** The worker's face, for a line of words about it. Decorative: `alt=""`. */
export function WorkerFace({ of }: { readonly of: Drawn }): React.ReactElement {
  const look = useLook(of);
  return <img className={look.tint === true ? "worker-face is-ultracode" : "worker-face"} data-testid="worker-face" data-look={look.name} src={look.parts.head.src} alt="" draggable={false} />;
}

/** The filter behind the tint. Mounted once, by App. */
export function UltracodeTint(): React.ReactElement {
  return (
    <svg className="tint-defs" width="0" height="0" aria-hidden focusable="false">
      <filter id="ultracode-tint" colorInterpolationFilters="sRGB">
        {/* Mixes 18% of --ultracode (#8f5bdb: 143 / 91 / 219) into every pixel and leaves alpha alone, so nothing is painted outside a piece. */}
        <feColorMatrix type="matrix" values="0.82 0 0 0 0.101  0 0.82 0 0 0.064  0 0 0.82 0 0.155  0 0 0 1 0" />
      </filter>
    </svg>
  );
}
