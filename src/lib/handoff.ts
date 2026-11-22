import type { Handoff } from "../types";

const ELSEWHERE = "This agent runs where the app found no terminal window to bring forward. Answer it where it runs.";

/** What to say about where a question is answered, and the button that goes there, if any. */
export function handoffWords(handoff: Handoff | null): { readonly hint: string; readonly action: string | null } {
  switch (handoff?.kind) {
    case "docked":
      return { hint: "This agent runs in one of the app's terminals. Answer it there. Opening the terminal does not answer it.", action: "Open its terminal" };
    case "window":
      return handoff.tab
        ? { hint: `This agent runs in ${handoff.app}, outside the app. Answer it there. The app can bring its tab to the front.`, action: `Bring its ${handoff.app} tab to the front` }
        : {
            hint: `This agent runs in ${handoff.app}, outside the app. Answer it there. The app can bring ${handoff.app} to the front, but cannot pick the tab.`,
            action: `Bring ${handoff.app} to the front`,
          };
    case "read-only":
      return { hint: "The building only reads this session. Answer it where it runs.", action: null };
    case "chat":
      return { hint: "Answer it in its conversation.", action: "Open the request" };
    case "conversation":
      return { hint: "Reply in its conversation.", action: "Open its conversation" };
    case "elsewhere":
    case undefined:
      return { hint: ELSEWHERE, action: null };
  }
}
