import { useCallback, useEffect, useRef, useState } from "react";
import type { EditorList } from "../types";

/**
 * Loads the editor list on mount and again on every `reload()`. Only the
 * newest request's answer is applied, and a failed load keeps what was shown.
 * `load` must be stable.
 */
export function useEditors(load: () => Promise<EditorList>): { readonly list: EditorList | null; readonly reload: () => void } {
  const [list, setList] = useState<EditorList | null>(null);
  const [attempt, setAttempt] = useState(0);
  const newest = useRef(0);

  useEffect(() => {
    newest.current += 1;
    const mine = newest.current;
    load()
      .then((next) => {
        if (newest.current === mine) setList(next);
      })
      .catch(() => {});
    return () => {
      newest.current += 1;
    };
  }, [load, attempt]);

  const reload = useCallback(() => setAttempt((n) => n + 1), []);
  return { list, reload };
}
