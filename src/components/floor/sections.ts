/** The manager's panels, in the sketch's order. `setup` exists only on the setup floor. */
export type Section = "setup" | "info" | "decisions" | "human-todo" | "todo" | "spend" | "signed-out";

export const SECTIONS: readonly { readonly id: Section; readonly label: string }[] = [
  { id: "info", label: "Info" },
  { id: "decisions", label: "Decisions" },
  { id: "human-todo", label: "Human TODO" },
  { id: "todo", label: "TODO" },
  { id: "spend", label: "Spend" },
  { id: "signed-out", label: "Signed out" },
];

const SETUP = { id: "setup", label: "Setup" } as const;

/** The rail of a floor: the setup floor leads with its Setup panel. */
export function sectionsFor(setupFloor: boolean): readonly { readonly id: Section; readonly label: string }[] {
  return setupFloor ? [SETUP, ...SECTIONS] : SECTIONS;
}

export function sectionLabel(section: Section): string {
  return sectionsFor(true).find((s) => s.id === section)?.label ?? section;
}
