import { createContext } from "react";

export interface ManagerActions {
  /** Resolves once the manager has started. The outcome is also a toast. */
  readonly hire: (projectId: string) => Promise<void>;
  readonly setPaused: (projectId: string, paused: boolean) => Promise<void>;
  readonly setHiring: (projectId: string, allowed: boolean) => Promise<void>;
  /** A failure is a toast. */
  readonly withdraw: (projectId: string, noteId: string) => void;
  readonly openSettings: () => void;
}

/** Provided by App. A missing provider is not an error: the controls then render nothing. */
export const ManagerContext = createContext<ManagerActions | null>(null);
