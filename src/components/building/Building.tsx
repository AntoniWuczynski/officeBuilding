import { STOREY } from "../../lib/layout";
import { Billboard, IsoBox, Wall } from "../../scene/Iso";
import type { StyleWithVars } from "../../scene/Iso";
import { Storey } from "./Storey";
import type { FloorModel } from "./Storey";

interface BuildingProps {
  readonly floors: readonly FloorModel[];
  readonly width: number;
  readonly onEnter: (projectId: string, from: HTMLElement) => void;
  readonly onAddFloor: () => void;
}

/** Height of the whole building in world pixels, roof included. */
export function buildingHeight(floorCount: number): number {
  return (floorCount + 1) * STOREY.height + 32;
}

/** The cutaway: lobby at the bottom, one storey per project, roof on top. */
export function Building({ floors, width, onEnter, onAddFloor }: BuildingProps): React.ReactElement {
  const n = floors.length;
  return (
    <>
      <Lobby width={width} />
      {floors.map((floor, k) => (
        <Storey key={floor.project.id} level={n - k} width={width} floor={floor} onEnter={onEnter} />
      ))}
      <Roof width={width} level={n + 1} onAddFloor={onAddFloor} />
    </>
  );
}

function Lobby({ width }: { readonly width: number }): React.ReactElement {
  const wallH = STOREY.height - STOREY.slab;
  const style: StyleWithVars = { "--z": "0px" };
  return (
    <div className="storey lobby" style={style}>
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

function Roof({ width, level, onAddFloor }: { readonly width: number; readonly level: number; readonly onAddFloor: () => void }): React.ReactElement {
  const z = level * STOREY.height;
  return (
    <>
      <IsoBox x={0} y={0} z={z} w={width} d={STOREY.depth} h={8} top="var(--roof)" front="var(--concrete)" side="var(--concrete-shade)" />
      <IsoBox x={22} y={12} z={z + 8} w={30} d={30} h={20} top="#A7B0B5" front="#8D969B" side="#7A8388" />
      <Billboard x={width / 2 + 20} y={STOREY.depth / 2} z={z + 8}>
        <span className="roof-sign">Office Building</span>
      </Billboard>
      <Billboard x={width - 30} y={STOREY.depth - 10} z={z + 8}>
        <button type="button" className="add-floor-sign" data-testid="add-floor" onClick={onAddFloor}>
          <span aria-hidden>+</span> Add a floor
        </button>
      </Billboard>
    </>
  );
}
