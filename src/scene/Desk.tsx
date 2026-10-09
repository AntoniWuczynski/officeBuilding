import type { SessionState } from "../types";
import { DESK } from "../lib/layout";
import { Billboard, IsoBox } from "./Iso";

interface DeskProps {
  readonly x: number;
  readonly y: number;
  readonly z: number;
  /** Drives the monitor's status LED; omitted for the manager's desk. */
  readonly led?: SessionState;
  /** A potted plant on the back of the desk: its agent works in a git worktree. */
  readonly plant?: boolean;
}

/** A timber desk with a monitor on its left end (the worker sits beside it, never behind it). */
export function Desk({ x, y, z, led, plant = false }: DeskProps): React.ReactElement {
  return (
    <>
      <IsoBox x={x} y={y} z={z} w={DESK.w} d={DESK.d} h={DESK.h} top="var(--timber)" front="var(--timber-front)" side="var(--timber-side)" />
      <IsoBox
        x={x + 3}
        y={y + 3}
        z={z + DESK.h}
        w={11}
        d={3}
        h={11}
        top="var(--monitor-top)"
        front="var(--monitor)"
        side="var(--monitor-side)"
        frontContent={<b className={led === undefined ? "led" : `led led-${led}`} />}
      />
      {/* The keyboard the worker behind the desk types on. */}
      <IsoBox x={x + 20} y={y + 1} z={z + DESK.h} w={12} d={5} h={1.2} top="var(--monitor-top)" front="var(--monitor)" side="var(--monitor-side)" />
      {plant ? <Plant x={x + 15} y={y + 2} z={z + DESK.h} /> : null}
    </>
  );
}

interface PlantProps {
  readonly x: number;
  readonly y: number;
  readonly z: number;
  /** Shrinks it for a helper's tiny desk. */
  readonly scale?: number;
}

/** A potted plant on a desk: its agent works in a git worktree. */
export function Plant({ x, y, z, scale = 1 }: PlantProps): React.ReactElement {
  const pot = 4 * scale;
  return (
    <>
      <IsoBox x={x} y={y} z={z} w={pot} d={pot} h={pot} top="var(--soil)" front="var(--pot-front)" side="var(--pot-side)" />
      <Billboard x={x + pot / 2} y={y + pot / 2} z={z + pot}>
        <span className="desk-plant" data-testid="desk-plant" style={scale === 1 ? undefined : { transform: `scale(${scale})` }} />
      </Billboard>
    </>
  );
}
