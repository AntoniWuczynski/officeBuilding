import { useRef } from "react";
import type { FloorLight, Project, Session } from "../../types";
import type { Counts } from "../../state/selectors";
import { lightWords, poseForState } from "../../lib/animation";
import { STOREY, floorSpotOf, seatOf, storeyDeskCount, storeyDeskX } from "../../lib/layout";
import { lookFor, MANAGER_LOOK } from "../../lib/looks";
import { Billboard, IsoBox, Wall } from "../../scene/Iso";
import type { StyleWithVars } from "../../scene/Iso";
import { Desk } from "../../scene/Desk";
import { Person } from "../../scene/Person";

export interface FloorModel {
  readonly project: Project;
  readonly sessions: readonly Session[];
  readonly light: FloorLight;
  readonly counts: Counts;
  /** Questions waiting on the human for this floor. */
  readonly pending: number;
  /** Search does not match: the floor dims but keeps its place in the building. */
  readonly dimmed: boolean;
}

interface StoreyProps {
  readonly level: number;
  readonly width: number;
  readonly floor: FloorModel;
  readonly onEnter: (projectId: string, from: HTMLElement) => void;
}

const WALL_H = STOREY.height - STOREY.slab;

/** One storey of the cutaway: a room you can see into. Click it to go inside. */
export function Storey({ level, width, floor, onEnter }: StoreyProps): React.ReactElement {
  const ref = useRef<HTMLDivElement>(null);
  const { project, sessions, light, counts, pending, dimmed } = floor;
  const shown = sessions.slice(0, storeyDeskCount(sessions.length));
  const overflow = sessions.length - shown.length;
  const managerX = width - STOREY.managerWidth;
  const style: StyleWithVars = { "--z": `${level * STOREY.height}px` };

  return (
    <div
      ref={ref}
      className={`storey light-${light}${dimmed ? " is-dim" : ""}`}
      style={style}
      data-testid="storey"
      data-project={project.id}
      data-light={light}
      onClick={() => {
        if (ref.current !== null) onEnter(project.id, ref.current);
      }}
    >
      {/* A still, invisible volume: when the room slides out from under the
          pointer, the pointer lands on this, so the hover (and the slide) hold. */}
      <IsoBox
        x={0}
        y={0}
        w={width}
        d={STOREY.depth}
        h={STOREY.height - 2}
        top="transparent"
        front="transparent"
        side="transparent"
        className="storey-hit"
      />
      <div className="storey-slide">
      <IsoBox
        x={0}
        y={0}
        w={width}
        d={STOREY.depth}
        h={STOREY.slab}
        top="var(--boards)"
        front="var(--concrete)"
        side="var(--concrete-shade)"
        frontContent={<b className="status-strip" />}
      />
      <Wall axis="back" x={0} y={0} z={STOREY.slab} length={width} height={WALL_H} background="var(--window-wall)" className="wall-windows" />
      <Wall axis="left" x={0} y={0} z={STOREY.slab} length={STOREY.depth} height={WALL_H} background="var(--plaster-shade)" />

      {shown.map((s, i) => (
        <Seat key={s.id} session={s} x={storeyDeskX(i)} />
      ))}
      {overflow > 0 ? (
        <Billboard x={storeyDeskX(shown.length - 1) + 44} y={STOREY.deskY + 8} z={STOREY.slab + 20}>
          <span className="overflow-sign">+{overflow}</span>
        </Billboard>
      ) : null}

      {/* manager's glass office at the right end */}
      <Wall axis="left" x={managerX} y={0} z={STOREY.slab} length={STOREY.depth} height={WALL_H} background="var(--glass)" className="glass" />
      <Desk x={width - 54} y={STOREY.deskY} z={STOREY.slab} />
      <Billboard x={width - 29} y={STOREY.deskY - 2} z={STOREY.slab}>
        <Person look={MANAGER_LOOK} pose="idle" />
      </Billboard>
      {pending > 0 ? (
        <Billboard x={width - 12} y={STOREY.depth - 16} z={STOREY.slab + 16}>
          <span className="memo-badge" aria-hidden>{pending}</span>
        </Billboard>
      ) : null}
      </div>

      <Billboard x={0} y={STOREY.depth} z={STOREY.height * 0.5} className="nameplate-anchor">
        <button
          type="button"
          className="nameplate"
          data-testid="floor-nameplate"
          aria-label={`${project.name}: ${counts.active} of ${counts.total} active, ${lightWords(light)}${pending > 0 ? `, ${pending} waiting on you` : ""}. Go to floor.`}
        >
          <i className="lamp" aria-hidden />
          <span className="nameplate-name">{project.name}</span>
          <span className="nameplate-count">
            {counts.active}/{counts.total}
          </span>
        </button>
      </Billboard>
    </div>
  );
}

function Seat({ session, x }: { readonly session: Session; readonly x: number }): React.ReactElement {
  const desk = { x, y: STOREY.deskY };
  const spot = session.state === "error" ? floorSpotOf(desk, STOREY.depth - 3) : seatOf(desk);
  return (
    <>
      <Desk x={x} y={STOREY.deskY} z={STOREY.slab} led={session.state} />
      <Billboard x={spot.x} y={spot.y} z={STOREY.slab}>
        <Person look={lookFor(session.id)} pose={poseForState(session.state)} />
      </Billboard>
    </>
  );
}
