/** What a room is built of: an office floor, or the setup floor's plant room. */
export interface Materials {
  readonly floor: string;
  readonly backWall: string;
  readonly leftWall: string;
  /** `wall-windows` is dimmed when nobody works there. */
  readonly backClass: "wall-windows" | "wall-pipes";
}

const OFFICE: Materials = { floor: "var(--boards)", backWall: "var(--window-wall)", leftWall: "var(--plaster-shade)", backClass: "wall-windows" };
const PLANT: Materials = { floor: "var(--plant-floor)", backWall: "var(--pipe-wall)", leftWall: "var(--pegboard)", backClass: "wall-pipes" };

export function materialsFor(plant: boolean): Materials {
  return plant ? PLANT : OFFICE;
}
