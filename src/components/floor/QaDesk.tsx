import type { QaFloor } from "../../types";
import { qaLine, STAMP_WORDS } from "../../state/selectors";
import { DESK, ROOM, qaDeskSlot } from "../../lib/layout";
import { Billboard } from "../../scene/Iso";
import { Desk, MagnifyingGlass } from "../../scene/Desk";
import { QaStamp } from "../qa/QaStamp";

interface Props {
  readonly qa: QaFloor;
  readonly depth: number;
  /** The reviewer sits here, so its placement draws the desk. */
  readonly occupied: boolean;
  readonly now: Date;
  readonly onOpen: () => void;
}

/** The QA desk in the floor's front-left corner: a magnifying glass, and a tag that opens the Info panel. */
export function QaDesk({ qa, depth, occupied, now, onOpen }: Props): React.ReactElement {
  const slot = qaDeskSlot(depth);
  const spoken = qa.stamp === null ? "" : ` ${STAMP_WORDS[qa.stamp].spoken}.`;
  return (
    <>
      {occupied ? null : <Desk x={slot.x} y={slot.y} z={ROOM.slab} />}
      <MagnifyingGlass x={slot.x + 22} y={slot.y + 9} z={ROOM.slab + DESK.h} />
      <Billboard x={slot.x + DESK.w / 2} y={slot.y} z={ROOM.slab + DESK.h + 44}>
        <button type="button" className="memo-tag qa-tag" data-testid="object-qa" aria-label={`QA desk: ${qaLine(qa, now)}${spoken} Open Info.`} onClick={onOpen}>
          QA
          {qa.stamp === null ? null : <QaStamp verdict={qa.stamp} />}
        </button>
      </Billboard>
    </>
  );
}
