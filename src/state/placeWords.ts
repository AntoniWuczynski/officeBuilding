import type { ViewMode } from "./useViewMode";

/** The words for a project's place in the current overview. The office set is the original text, unchanged. */
export interface PlaceWords {
  readonly noun: string;
  /** The back button on a project's screen. */
  readonly back: string;
  /** The overview with no projects in it. */
  readonly emptyNote: string;
  readonly addTitle: string;
  readonly addHint: string;
  readonly addAction: string;
  readonly addFailed: string;
  readonly addedToast: (name: string, known: boolean) => string;
  readonly removeTitle: (name: string) => string;
  readonly removeVerb: string;
  readonly removeBusy: string;
  readonly removeNote: string;
  readonly removeFailed: string;
  readonly removeLabel: (name: string) => string;
  readonly removeToast: (name: string | undefined) => string;
}

const OFFICE: PlaceWords = {
  noun: "floor",
  back: "Building",
  emptyNote: "No projects yet. When an agent starts work in a repo, its floor appears here, or add a floor from the roof.",
  addTitle: "Add a floor",
  addHint: "A floor is a project folder. Agents that start work there take a desk on it; until then its lights stay off.",
  addAction: "Add floor",
  addFailed: "Could not add the floor",
  addedToast: (name, known) => (known ? `${name} already has a floor.` : `Added the ${name} floor.`),
  removeTitle: (name) => `Demolish ${name}?`,
  removeVerb: "Demolish",
  removeBusy: "Demolishing",
  removeNote:
    "The floor comes back if an agent starts work in this folder again, or if you add it from the roof. A rebuilt floor keeps its sign-out sheet, so anyone who left in the last 12 hours can be resumed from there.",
  removeFailed: "Could not demolish the floor",
  removeLabel: (name) => `Demolish ${name}`,
  removeToast: (name) => (name === undefined ? "Demolished the floor." : `Demolished the ${name} floor.`),
};

const ISLANDS: PlaceWords = {
  noun: "island",
  back: "Islands",
  emptyNote: "No projects yet. When an agent starts work in a repo, its island appears here, or add an island from the list.",
  addTitle: "Add an island",
  addHint: "An island is a project folder. Agents that start work there take a seat on it; until then its light stays off.",
  addAction: "Add island",
  addFailed: "Could not add the island",
  addedToast: (name, known) => (known ? `${name} already has an island.` : `Added the ${name} island.`),
  removeTitle: (name) => `Sink ${name}?`,
  removeVerb: "Sink",
  removeBusy: "Sinking",
  removeNote:
    "The island rises again if an agent starts work in this folder, or if you add it from the list. A returning island keeps its sign-out sheet, so anyone who left in the last 12 hours can be resumed from there.",
  removeFailed: "Could not sink the island",
  removeLabel: (name) => `Sink ${name}`,
  removeToast: (name) => (name === undefined ? "Sank the island." : `Sank the ${name} island.`),
};

export const PLACE: Readonly<Record<ViewMode, PlaceWords>> = { office: OFFICE, islands: ISLANDS };
