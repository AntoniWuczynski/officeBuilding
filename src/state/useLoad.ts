import { useCallback, useEffect, useState } from "react";
import { message } from "../lib/errors";

export type Loaded<T> =
  | { readonly phase: "reading" }
  | { readonly phase: "failed"; readonly message: string }
  | { readonly phase: "ready"; readonly value: T };

/** Runs `load` on mount and whenever `load` changes (keep it stable). A result that arrives after unmount is dropped. */
export function useLoad<T>(load: () => Promise<T>): { readonly state: Loaded<T>; readonly set: (value: T) => void } {
  const [state, setState] = useState<Loaded<T>>({ phase: "reading" });

  useEffect(() => {
    let live = true;
    setState({ phase: "reading" });
    load().then(
      (value) => live && setState({ phase: "ready", value }),
      (err: unknown) => live && setState({ phase: "failed", message: message(err) }),
    );
    return () => {
      live = false;
    };
  }, [load]);

  const set = useCallback((value: T) => setState({ phase: "ready", value }), []);
  return { state, set };
}
