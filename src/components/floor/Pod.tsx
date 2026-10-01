import type { Helper } from "../../types";
import { POD, podFor } from "../../lib/layout";
import type { Slot } from "../../lib/layout";
import { poseForState } from "../../lib/animation";
import { lookFor } from "../../lib/looks";
import { Billboard, IsoBox } from "../../scene/Iso";
import type { StyleWithVars } from "../../scene/Iso";
import { Person } from "../../scene/Person";

interface Props {
  /** The session's own desk; the pod sits beside it. */
  readonly desk: Slot;
  readonly z: number;
  readonly helpers: readonly Helper[];
}

/**
 * A session's helpers (sub-agents) at tiny desks beside its own. The desks
 * shrink as the team grows, drop their people once too small to seat one, and
 * past POD.cap the rest show as a count.
 */
export function Pod({ desk, z, helpers }: Props): React.ReactElement | null {
  if (helpers.length === 0) return null;
  const pod = podFor(desk, helpers.length);
  const w = pod.cell * 0.78;
  const d = pod.cell * 0.5;
  const h = Math.max(1.5, pod.cell * 0.24);
  const seated = pod.cell >= POD.personCell;
  const scale: StyleWithVars = { "--pod-scale": String(pod.cell / 40) };
  const bottom = Math.max(...pod.slots.map((s) => s.y)) + pod.cell;
  return (
    <div className="group pod" data-testid="pod">
      {pod.slots.map((slot, i) => {
        const helper = helpers[i];
        if (helper === undefined) return null;
        return (
          <div key={helper.id} className="group">
            <IsoBox
              x={slot.x}
              y={slot.y}
              z={z}
              w={w}
              d={d}
              h={h}
              top="var(--timber)"
              front="var(--timber-front)"
              side="var(--timber-side)"
              className="pod-desk"
              topContent={<b className={`pod-led led-${helper.state}`} data-testid="pod-desk" />}
            />
            {seated ? (
              <Billboard x={slot.x + w * 0.62} y={slot.y - 1} z={z}>
                <span className="pod-person" style={scale}>
                  <Person look={lookFor(helper.id)} pose={poseForState(helper.state)} />
                </span>
              </Billboard>
            ) : null}
          </div>
        );
      })}
      {pod.hidden === 0 ? null : (
        <Billboard x={pod.region.x + pod.region.w / 2} y={bottom + 4} z={z + 6}>
          <span className="pod-more" data-testid="pod-more">
            +{pod.hidden}
          </span>
        </Billboard>
      )}
    </div>
  );
}
