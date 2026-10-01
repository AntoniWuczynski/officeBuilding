import type { SessionState } from "../types";
import { DESK } from "../lib/layout";
import { IsoBox } from "./Iso";

interface DeskProps {
  readonly x: number;
  readonly y: number;
  readonly z: number;
  /** Drives the monitor's status LED; omitted for the manager's desk. */
  readonly led?: SessionState;
}

/** A timber desk with a monitor on its left end (the worker sits beside it, never behind it). */
export function Desk({ x, y, z, led }: DeskProps): React.ReactElement {
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
    </>
  );
}
