import { createContext } from "react";
import type { Editor, EditorList, Place } from "../../types";

export interface EditorActions {
  readonly list: EditorList | null;
  /** Open `place` in `editor` (null: the default) and report the outcome as a toast. `what` names the place in it. */
  readonly open: (place: Place, editor: Editor | null, what: string) => void;
}

/** Provided by App. A missing provider is not an error: the controls then render nothing. */
export const EditorContext = createContext<EditorActions | null>(null);
