import type { Spend, TodoItem } from "../../types";
import type { Counts } from "../../state/selectors";
import { formatTokens, formatUsd } from "../../state/selectors";
import { ROOM } from "../../lib/layout";
import { MANAGER_LOOK } from "../../lib/looks";
import { Billboard, IsoBox, Wall } from "../../scene/Iso";
import { Desk } from "../../scene/Desk";
import { Person } from "../../scene/Person";
import type { Section } from "./sections";

/** Everything the room's objects display; each object opens its own panel. */
export interface RoomObjectsModel {
  readonly projectName: string;
  readonly path: string;
  readonly counts: Counts;
  readonly decisionsWaiting: number;
  readonly humanTodos: readonly TodoItem[];
  readonly aiTodos: readonly TodoItem[];
  readonly spend: Spend;
}

interface ObjectsProps {
  readonly model: RoomObjectsModel;
  readonly depth: number;
  readonly onOpen: (section: Section) => void;
}

const NOTE_COLOURS = ["var(--note-yellow)", "var(--note-blue)", "var(--note-pink)"] as const;

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** Back wall: windows, the AI's whiteboard and the spend meter. */
export function BackWall({ model, onOpen }: ObjectsProps): React.ReactElement {
  const open = model.aiTodos.filter((t) => !t.done);
  const shown = model.aiTodos.slice(0, 3);
  return (
    <Wall axis="back" x={0} y={0} z={ROOM.slab} length={ROOM.width} height={ROOM.wallHeight} background="var(--window-wall)" className="wall-windows">
      <button
        type="button"
        className="whiteboard"
        data-testid="object-todo"
        aria-label={`AI TODO whiteboard: ${plural(open.length, "open item", "open items")}. Open TODO.`}
        onClick={() => onOpen("todo")}
      >
        <b>TODO</b>
        {shown.length === 0 ? <span className="whiteboard-line">Nothing on the board.</span> : null}
        {shown.map((t) => (
          <span key={t.id} className={t.done ? "whiteboard-line is-done" : "whiteboard-line"}>
            {t.text}
          </span>
        ))}
        {model.aiTodos.length > shown.length ? (
          <span className="whiteboard-more">{model.aiTodos.length - shown.length} more</span>
        ) : null}
      </button>
      <button
        type="button"
        className="meter"
        data-testid="object-spend"
        aria-label={`Spend on this floor: ${formatUsd(model.spend.usd)}. Open Spend.`}
        onClick={() => onOpen("spend")}
      >
        <span className="meter-label">Spend</span>
        <b>{formatUsd(model.spend.usd)}</b>
        <span className="meter-tokens">{formatTokens(model.spend.tokens)}</span>
      </button>
    </Wall>
  );
}

/** Left wall: the human's cork board near the front, the brass plaque further back. */
export function LeftWall({ model, depth, onOpen }: ObjectsProps): React.ReactElement {
  const open = model.humanTodos.filter((t) => !t.done);
  return (
    <Wall axis="left-upright" x={0} y={depth} z={ROOM.slab} length={depth} height={ROOM.wallHeight} background="var(--plaster-shade)">
      <button
        type="button"
        className="corkboard"
        data-testid="object-human-todo"
        aria-label={`Human TODO board: ${plural(open.length, "open item", "open items")}. Open Human TODO.`}
        onClick={() => onOpen("human-todo")}
      >
        <b>Human TODO</b>
        {open.length === 0 ? <span className="corkboard-empty">All done.</span> : null}
        {open.slice(0, 3).map((t, i) => (
          <span key={t.id} className="sticky" style={{ background: NOTE_COLOURS[i % NOTE_COLOURS.length] }}>
            {t.text}
          </span>
        ))}
      </button>
      <button
        type="button"
        className="plaque"
        data-testid="object-info"
        style={{ left: depth - 118 }}
        aria-label={`Floor plaque: ${model.projectName}. Open Info.`}
        onClick={() => onOpen("info")}
      >
        <b>{model.projectName}</b>
        <span>
          {model.counts.active} of {model.counts.total} active
        </span>
      </button>
    </Wall>
  );
}

/** The manager's glass office: desk, manager and the pile of decisions waiting. */
export function ManagerOffice({ model, onOpen }: ObjectsProps): React.ReactElement {
  const x = ROOM.managerX;
  const deskX = x + 46;
  const waiting = model.decisionsWaiting;
  return (
    <>
      <Desk x={deskX} y={40} z={ROOM.slab} />
      <Billboard x={deskX + 25} y={38} z={ROOM.slab}>
        <Person look={MANAGER_LOOK} pose="idle" />
      </Billboard>
      {Array.from({ length: Math.min(waiting, 4) }, (_, i) => (
        <IsoBox
          key={i}
          x={deskX + 18 + (i % 2)}
          y={44 + (i % 2)}
          z={ROOM.slab + 12 + i * 3}
          w={12}
          d={9}
          h={3}
          top="var(--paper)"
          front="var(--rule)"
          side="var(--concrete)"
        />
      ))}
      <Wall axis="back" x={x} y={ROOM.managerDepth} z={ROOM.slab} length={ROOM.width - x} height={ROOM.glassHeight} background="var(--glass)" className="glass">
        <span className="glass-label">Manager</span>
      </Wall>
      <Wall axis="left" x={x} y={0} z={ROOM.slab} length={ROOM.managerDepth} height={ROOM.glassHeight} background="var(--glass)" className="glass" />
      <Billboard x={deskX + 22} y={50} z={ROOM.slab + 24}>
        <button
          type="button"
          className={waiting > 0 ? "memo-tag has-waiting" : "memo-tag"}
          data-testid="object-decisions"
          aria-label={`${plural(waiting, "decision", "decisions")} waiting on you. Open Decisions.`}
          onClick={() => onOpen("decisions")}
        >
          {waiting > 0 ? (
            <>
              <b>{waiting}</b> {waiting === 1 ? "decision" : "decisions"} waiting
            </>
          ) : (
            "No decisions waiting"
          )}
        </button>
      </Billboard>
    </>
  );
}
