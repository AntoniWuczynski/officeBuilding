import type { ConversationEntry } from "../../types";
import { DiffView } from "./DiffView";
import { LongText } from "./LongText";

const WORDS = {
  pending: "Waiting",
  running: "Running",
  done: "Done",
  failed: "Failed",
} as const;

/** One tool call, closed until opened: what it printed as text, and each file it changed as a diff. */
export function ToolCard({ entry, busy }: { readonly entry: Extract<ConversationEntry, { kind: "tool" }>; readonly busy: boolean }): React.ReactElement {
  const unfinished = entry.status === "pending" || entry.status === "running";
  return (
    <details className={`chat-tool is-${entry.status}`}>
      <summary>
        <span className="chat-tool-status">{unfinished && !busy ? "Stopped" : WORDS[entry.status]}</span> {entry.title}
      </summary>
      {entry.detail === "" ? null : <LongText className="chat-output" text={entry.detail} />}
      {entry.diffs.map((d, i) => (
        <DiffView key={`${i}-${d.path}`} diff={d} />
      ))}
    </details>
  );
}
