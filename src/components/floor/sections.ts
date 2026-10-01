/** The manager's panels, in the sketch's order. */
export type Section = "info" | "decisions" | "human-todo" | "todo" | "spend" | "signed-out";

export const SECTIONS: readonly { readonly id: Section; readonly label: string }[] = [
  { id: "info", label: "Info" },
  { id: "decisions", label: "Decisions" },
  { id: "human-todo", label: "Human TODO" },
  { id: "todo", label: "TODO" },
  { id: "spend", label: "Spend" },
  { id: "signed-out", label: "Signed out" },
];

export function sectionLabel(section: Section): string {
  return SECTIONS.find((s) => s.id === section)?.label ?? section;
}
