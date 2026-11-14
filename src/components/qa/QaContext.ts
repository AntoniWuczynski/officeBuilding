import { createContext } from "react";
import type { Review } from "../../types";

export interface QaActions {
  /** Resolves once the floor is switched. The outcome is also a toast. */
  readonly setFloor: (projectId: string, enabled: boolean) => Promise<void>;
  /** Ask GitHub again now. A failure is a toast. */
  readonly recheck: () => void;
  /** Open a review in the browser. A failure is a toast. */
  readonly openReview: (projectId: string, review: Review) => void;
  readonly openSettings: () => void;
}

/** Provided by App. A missing provider is not an error: the controls then render nothing. */
export const QaContext = createContext<QaActions | null>(null);
