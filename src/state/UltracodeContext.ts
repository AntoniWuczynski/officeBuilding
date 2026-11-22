import { createContext } from "react";

/** Claude Code sessions the app itself hired with ultracode on, by session id (`OfficeSnapshot.ultracode`). */
export const UltracodeContext = createContext<ReadonlySet<string>>(new Set());
