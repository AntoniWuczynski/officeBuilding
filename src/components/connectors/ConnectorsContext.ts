import { createContext } from "react";
import type { ConnectorKind, FloorLink, LinearKeyStatus, LinearTeam } from "../../types";

export interface ConnectorActions {
  readonly keyStatus: () => Promise<LinearKeyStatus>;
  readonly setKey: (key: string | null) => Promise<LinearKeyStatus>;
  readonly teams: () => Promise<readonly LinearTeam[]>;
  /** Resolves once the floor is linked. The outcome is also a toast. */
  readonly connect: (projectId: string, link: FloorLink) => Promise<void>;
  readonly disconnect: (projectId: string, connector: ConnectorKind) => Promise<void>;
  /** Ask again now. A failure is a toast. */
  readonly recheck: () => void;
  /** Open a Linear item in the browser. A failure is a toast. */
  readonly openLink: (todoId: string) => void;
  readonly openSettings: () => void;
}

/** Provided by App. A missing provider is not an error: the controls then render nothing. */
export const ConnectorsContext = createContext<ConnectorActions | null>(null);
