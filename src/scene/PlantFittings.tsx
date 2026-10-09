import { IsoBox } from "./Iso";

/** A duct running along the top of the back wall, from `x` for `length`. */
export function Duct({ x, length, z }: { readonly x: number; readonly length: number; readonly z: number }): React.ReactElement {
  return <IsoBox x={x} y={2} z={z} w={length} d={10} h={10} top="var(--duct-top)" front="var(--duct)" side="var(--duct-side)" className="plant-duct" />;
}

const TOOLBOX = { w: 22, d: 12, h: 10 } as const;

/**
 * The middle of the toolbox's lid beside a tank at (x, y). It stands forward of
 * the tank's front corner, so a tag over it is drawn in front of the tank.
 */
export function toolboxTop(x: number, y: number, size: number): { readonly x: number; readonly y: number; readonly h: number } {
  return { x: x + size + 24 + TOOLBOX.w / 2, y: y + 16 + TOOLBOX.d / 2, h: TOOLBOX.h };
}

/** A tank with a toolbox beside it, standing on the floor at (x, y). */
export function Tank({ x, y, z, size, height }: { readonly x: number; readonly y: number; readonly z: number; readonly size: number; readonly height: number }): React.ReactElement {
  const box = toolboxTop(x, y, size);
  return (
    <>
      <IsoBox x={x} y={y} z={z} w={size} d={size} h={height} top="var(--tank-top)" front="var(--tank)" side="var(--tank-side)" className="plant-tank" />
      <IsoBox x={box.x - TOOLBOX.w / 2} y={box.y - TOOLBOX.d / 2} z={z} w={TOOLBOX.w} d={TOOLBOX.d} h={TOOLBOX.h} top="var(--signage)" front="var(--signage-edge)" side="var(--signage-edge)" className="plant-toolbox" />
    </>
  );
}
