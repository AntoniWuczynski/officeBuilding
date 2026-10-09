import { STOREY } from "../../lib/layout";
import { Billboard, IsoBox, Wall } from "../../scene/Iso";
import type { StyleWithVars } from "../../scene/Iso";
import { EditableSign } from "./EditableSign";
import { Storey } from "./Storey";
import type { FloorModel } from "./Storey";
import type { Present } from "../../state/usePresence";

interface BuildingProps {
  /** Top floor first, including floors still going up or coming down. */
  readonly floors: readonly Present<FloorModel>[];
  readonly animate: boolean;
  readonly width: number;
  readonly name: string;
  readonly onRename: (name: string) => void;
  readonly onEnter: (projectId: string, from: HTMLElement) => void;
  readonly onAddFloor: () => void;
  readonly onDemolish: (projectId: string) => void;
  readonly onGone: (projectId: string) => void;
}

/** Height of the whole building in world pixels, roof included. */
export function buildingHeight(floorCount: number): number {
  return (floorCount + 1) * STOREY.height + 32;
}

/**
 * The cutaway: the setup floor's plant room in the basement, then the lobby,
 * one storey per project, and the roof on top.
 */
export function Building({ floors, animate, width, name, onRename, onEnter, onAddFloor, onDemolish, onGone }: BuildingProps): React.ReactElement {
  const n = floors.length;
  const basement = floors.filter((f) => f.item.plant);
  const above = floors.filter((f) => !f.item.plant);
  const lobby = basement.length;
  const storey = ({ key, item, phase }: Present<FloorModel>, level: number): React.ReactElement => (
    <Storey
      key={key}
      level={level}
      width={width}
      floor={item}
      phase={phase}
      animate={animate}
      onEnter={onEnter}
      onDemolish={onDemolish}
      onGone={onGone}
    />
  );
  return (
    <>
      {basement.map((f) => storey(f, 0))}
      <Lobby width={width} level={lobby} />
      {above.map((f, k) => storey(f, n - k))}
      <Roof width={width} level={n + 1} name={name} onRename={onRename} onAddFloor={onAddFloor} />
    </>
  );
}

function Lobby({ width, level }: { readonly width: number; readonly level: number }): React.ReactElement {
  const wallH = STOREY.height - STOREY.slab;
  const style: StyleWithVars = { "--z": `${level * STOREY.height}px` };
  return (
    <div className={level > 0 ? "storey lobby is-ground" : "storey lobby"} style={style}>
      <IsoBox x={0} y={0} w={width} d={STOREY.depth} h={STOREY.slab} top="var(--boards)" front="var(--concrete)" side="var(--concrete-shade)" />
      <Wall axis="back" x={0} y={0} z={STOREY.slab} length={width} height={wallH} background="var(--window-wall)" className="wall-windows" />
      <Wall axis="left" x={0} y={0} z={STOREY.slab} length={STOREY.depth} height={wallH} background="var(--plaster-shade)" />
      <Wall axis="back" x={width / 2 - 24} y={0.5} z={STOREY.slab} length={48} height={40} background="var(--timber-front)" />
      <IsoBox x={width / 2 - 44} y={STOREY.depth - 34} z={STOREY.slab} w={88} d={14} h={14} top="var(--paper)" front="var(--rule)" side="var(--concrete)" />
      <Billboard x={0} y={STOREY.depth} z={STOREY.height * 0.5} className="nameplate-anchor">
        <span className="nameplate nameplate-static">
          <span className="nameplate-name">Lobby</span>
        </span>
      </Billboard>
    </div>
  );
}

function Roof({
  width,
  level,
  name,
  onRename,
  onAddFloor,
}: {
  readonly width: number;
  readonly level: number;
  readonly name: string;
  readonly onRename: (name: string) => void;
  readonly onAddFloor: () => void;
}): React.ReactElement {
  // Positioned like a storey, so it rises and settles with the floors below it.
  const style: StyleWithVars = { "--z": `${level * STOREY.height}px` };
  return (
    <div className="roof" style={style}>
      <IsoBox x={0} y={0} z={0} w={width} d={STOREY.depth} h={8} top="var(--roof)" front="var(--concrete)" side="var(--concrete-shade)" />
      <IsoBox x={22} y={12} z={8} w={30} d={30} h={20} top="#A7B0B5" front="#8D969B" side="#7A8388" />
      <Billboard x={width / 2 + 20} y={STOREY.depth / 2} z={8}>
        <EditableSign name={name} onRename={onRename} />
      </Billboard>
      <Billboard x={width - 30} y={STOREY.depth - 10} z={8}>
        <button type="button" className="add-floor-sign" data-testid="add-floor" onClick={onAddFloor}>
          <span aria-hidden>+</span> Add a floor
        </button>
      </Billboard>
    </div>
  );
}
