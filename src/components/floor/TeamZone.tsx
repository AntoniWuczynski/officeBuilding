import { ROOM, ZONE, partitionY } from "../../lib/layout";
import { Billboard, IsoBox, Wall } from "../../scene/Iso";

interface Props {
  /** How many desks the manager hired. */
  readonly count: number;
  /** The zone's desks. */
  readonly children: React.ReactNode;
}

/** The manager's team zone: a rug under its hired desks and a low glass partition between them and A Human's desks. */
export function TeamZone({ count, children }: Props): React.ReactElement | null {
  const line = partitionY(count);
  if (line === null) return null;
  const inset = 6;
  return (
    <div className="group manager-zone" role="group" aria-label="Manager's team: the desks the floor's manager hired">
      <IsoBox x={inset} y={inset} z={ROOM.slab} w={ROOM.managerX - 2 * inset} d={line - 2 * inset} h={1} top="var(--plaster)" front="var(--plaster-shade)" side="var(--plaster-shade)" />
      {children}
      <Wall axis="back" x={0} y={line} z={ROOM.slab} length={ROOM.width} height={ZONE.partitionHeight} background="var(--glass)" className="glass zone-partition" />
      <Billboard x={ROOM.colX0} y={line + 6} z={ROOM.slab + 16}>
        <span className="memo-tag zone-sign" data-testid="manager-zone">
          Manager&apos;s team
        </span>
      </Billboard>
    </div>
  );
}
