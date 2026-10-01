import { useCallback, useEffect, useState } from "react";
import type { DiscoveryApi } from "../api/DiscoveryApi";
import type { OfficeSnapshot } from "../types";

export type OfficeState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly snapshot: OfficeSnapshot }
  | { readonly status: "error"; readonly message: string };

/**
 * Subscribes a component tree to a {@link DiscoveryApi}. Loads the first
 * snapshot, then follows live pushes. A failed load surfaces as an error state
 * with a retry.
 */
export function useOffice(api: DiscoveryApi): { readonly state: OfficeState; readonly retry: () => void } {
  const [state, setState] = useState<OfficeState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    let unsubscribe: (() => void) | null = null;
    api
      .getSnapshot()
      .then((first) => {
        if (!live) return;
        setState({ status: "ready", snapshot: first });
        unsubscribe = api.subscribe((snapshot) => {
          setState({ status: "ready", snapshot });
        });
      })
      .catch((err: unknown) => {
        if (!live) return;
        setState({ status: "error", message: err instanceof Error ? err.message : String(err) });
      });
    return () => {
      live = false;
      unsubscribe?.();
    };
  }, [api, attempt]);

  const retry = useCallback(() => {
    setState({ status: "loading" });
    setAttempt((n) => n + 1);
  }, []);

  return { state, retry };
}
