import { CLAUDE_TERMINAL, STRUCTURED } from "../../api/mockConversation";
import type { ChatCapabilities, ConversationEntry } from "../../types";

export { CLAUDE_TERMINAL, STRUCTURED };

/** A session started outside the app: it can only be watched. */
export const READ_ONLY: ChatCapabilities = { compose: false, stop: false, end: false, approvals: "none", terminal: false };

/** One invented conversation that exercises every kind of entry. */
export const sampleTranscript: readonly ConversationEntry[] = [
  { kind: "user", id: "e1", text: "Please tidy the notes and run the tests." },
  {
    kind: "agent",
    id: "e2",
    text: "## Plan\n\nI will:\n\n- trim the notes\n- run the tests\n\n```ts\nconst answer = 42;\n```\n\nSee [the guide](https://example.test/guide).",
  },
  { kind: "thought", id: "e3", text: "The notes file is short, so one edit should do." },
  { kind: "tool", id: "e4", title: "pnpm test", status: "done", detail: "PASS  notes.test.ts\nPASS  tidy.test.ts\n2 passed", diffs: [] },
  {
    kind: "tool",
    id: "e5",
    title: "Edit notes.txt",
    status: "done",
    detail: "",
    diffs: [{ path: "notes.txt", text: "-old line\n+new line" }],
  },
  { kind: "tool", id: "e6", title: "pnpm lint", status: "failed", detail: "2 problems found", diffs: [] },
  { kind: "notice", id: "e7", text: "Turn stopped." },
];
