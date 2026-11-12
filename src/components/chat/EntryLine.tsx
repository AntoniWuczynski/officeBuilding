import { useState } from "react";
import type { ConversationEntry } from "../../types";
import { LONG_CHARS } from "./LongText";
import { Markdown } from "./Markdown";
import { ToolCard } from "./ToolCard";

const AGENT_MAX = LONG_CHARS * 2;

function AgentText({ text }: { readonly text: string }): React.ReactElement {
  const [whole, setWhole] = useState(false);
  if (whole || text.length <= AGENT_MAX) return <Markdown text={text} />;
  return (
    <>
      <Markdown text={text.slice(0, AGENT_MAX)} />
      <button type="button" className="link" onClick={() => setWhole(true)}>
        Show the whole message
      </button>
    </>
  );
}

/** One line of the log. Only an agent's message is Markdown: everything else is plain text. */
export function EntryLine({ entry, busy }: { readonly entry: ConversationEntry; readonly busy: boolean }): React.ReactElement {
  switch (entry.kind) {
    case "user":
      return <li className="conversation-user">{entry.text}</li>;
    case "agent":
      return (
        <li className="conversation-agent">
          <AgentText text={entry.text} />
        </li>
      );
    case "thought":
      return (
        <li className="conversation-thought">
          <details className="chat-thought">
            <summary>Thinking</summary>
            {entry.text}
          </details>
        </li>
      );
    case "notice":
      return <li className="conversation-notice">{entry.text}</li>;
    case "tool":
      return (
        <li className="conversation-tool">
          <ToolCard entry={entry} busy={busy} />
        </li>
      );
  }
}
