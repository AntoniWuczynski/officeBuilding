import type { Editor } from "../types";

/** The editors the building knows, in the backend's order, with the names it sends. */
export const MOCK_EDITORS: readonly { readonly editor: Editor; readonly label: string }[] = [
  { editor: "cursor", label: "Cursor" },
  { editor: "vs-code", label: "VS Code" },
  { editor: "zed", label: "Zed" },
  { editor: "devin-desktop", label: "Devin Desktop" },
];
