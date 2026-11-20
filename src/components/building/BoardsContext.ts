import { createContext } from "react";

/** The board tags you linked to each floor, and the way to change them; null where no one keeps them (tests, the floor screens' own harness). */
export interface BoardLinks {
  readonly linked: Readonly<Record<string, readonly string[]>>;
  readonly link: (projectId: string, tags: readonly string[]) => void;
}

export const BoardsContext = createContext<BoardLinks | null>(null);
