import { describe, expect, it } from "vitest";
import type { Handoff } from "../types";
import { handoffWords } from "./handoff";

const ELSEWHERE = "This agent runs where the app found no terminal window to bring forward. Answer it where it runs.";

const table: readonly (readonly [string, Handoff | null, string, string | null])[] = [
  ["docked", { kind: "docked" }, "This agent runs in one of the app's terminals. Answer it there. Opening the terminal does not answer it.", "Open its terminal"],
  [
    "a window with a tab",
    { kind: "window", app: "TERM-APP", tab: true },
    "This agent runs in TERM-APP, outside the app. Answer it there. The app can bring its tab to the front.",
    "Bring its TERM-APP tab to the front",
  ],
  [
    "a window without a tab",
    { kind: "window", app: "TERM-APP", tab: false },
    "This agent runs in TERM-APP, outside the app. Answer it there. The app can bring TERM-APP to the front, but cannot pick the tab.",
    "Bring TERM-APP to the front",
  ],
  ["conversation", { kind: "conversation" }, "Reply in its conversation.", "Open its conversation"],
  ["elsewhere", { kind: "elsewhere" }, ELSEWHERE, null],
  ["read-only", { kind: "read-only" }, "The building only reads this session. Answer it where it runs.", null],
  ["chat", { kind: "chat", approvalId: "7" }, "Answer it in its conversation.", "Open the request"],
  ["none", null, ELSEWHERE, null],
];

describe("handoffWords", () => {
  it.each(table)("says it for %s", (_name, handoff, hint, action) => {
    expect(handoffWords(handoff)).toEqual({ hint, action });
  });
});
