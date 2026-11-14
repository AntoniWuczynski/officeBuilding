import { createContext } from "react";
import type { HandoverApi } from "../../api/HandoverApi";

/** Provided by main. A missing provider is not an error: the strip then shows its line and no button. */
export const HandoverContext = createContext<HandoverApi | null>(null);
