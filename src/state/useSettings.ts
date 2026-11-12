import { useCallback, useEffect, useRef, useState } from "react";
import { message } from "../lib/errors";
import type { Settings, SettingsPatch } from "../types";

export type SettingsState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly settings: Settings; readonly saveError: string | null }
  | { readonly status: "error"; readonly message: string };

/**
 * Loads the settings and saves changes as they are made. A change shows at
 * once; only the newest save's outcome is applied, and a failed save reloads
 * what is really saved. `load` and `save` must be stable.
 */
export function useSettings(
  load: () => Promise<Settings>,
  save: (patch: SettingsPatch) => Promise<Settings>,
): { readonly state: SettingsState; readonly change: (patch: SettingsPatch) => void; readonly retry: () => void } {
  const [state, setShown] = useState<SettingsState>({ status: "loading" });
  const latest = useRef<SettingsState>(state);
  const setState = useCallback((next: SettingsState): void => {
    latest.current = next;
    setShown(next);
  }, []);
  const [attempt, setAttempt] = useState(0);
  const request = useRef(0);
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    load()
      .then((settings) => {
        if (live.current) setState({ status: "ready", settings, saveError: null });
      })
      .catch((err: unknown) => {
        if (live.current) setState({ status: "error", message: message(err) });
      });
    return () => {
      live.current = false;
    };
  }, [load, attempt, setState]);

  const change = useCallback(
    (patch: SettingsPatch) => {
      const current = latest.current;
      if (current.status !== "ready") return;
      request.current += 1;
      const mine = request.current;
      setState({ status: "ready", settings: { ...current.settings, ...patch }, saveError: null });
      const newest = (): boolean => live.current && request.current === mine;
      save(patch)
        .then((settings) => {
          if (newest()) setState({ status: "ready", settings, saveError: null });
        })
        .catch((err: unknown) => {
          if (!newest()) return;
          const saveError = message(err);
          load()
            .then((settings) => {
              if (newest()) setState({ status: "ready", settings, saveError });
            })
            .catch(() => {
              if (newest() && latest.current.status === "ready") setState({ ...latest.current, saveError });
            });
        });
    },
    [load, save, setState],
  );

  const retry = useCallback(() => {
    setState({ status: "loading" });
    setAttempt((n) => n + 1);
  }, [setState]);

  return { state, change, retry };
}
