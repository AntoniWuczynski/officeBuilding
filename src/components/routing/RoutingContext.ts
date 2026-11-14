import { createContext } from "react";
import type { RoutingApi } from "../../api/RoutingApi";

/** Provided by main. A missing provider is not an error: the controls then render nothing. */
export const RoutingContext = createContext<RoutingApi | null>(null);
