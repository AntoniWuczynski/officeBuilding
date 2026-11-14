import { useState } from "react";
import type { ChatCapabilities, QueuedNote } from "../../types";
import { QUEUE_WORDS } from "../../state/selectors";

/**
 * The message box under a conversation: the messages still waiting for the
 * agent, a box to write the next one, and Stop and End as the capabilities allow.
 */
export function Composer({
  capabilities,
  busy,
  queued,
  onSend,
  onWithdraw,
  onStop,
  onEnd,
}: {
  readonly capabilities: ChatCapabilities;
  readonly busy: boolean;
  readonly queued: readonly QueuedNote[];
  readonly onSend: (text: string) => Promise<void>;
  readonly onWithdraw: (id: string) => void;
  readonly onStop: () => void;
  readonly onEnd: () => void;
}): React.ReactElement {
  const [draft, setDraft] = useState("");
  // Set while a send is in flight, so a second press does not send the same message again.
  const [sending, setSending] = useState(false);
  const send = (): void => {
    const text = draft.trim();
    if (text === "" || sending) return;
    setSending(true);
    // The draft goes only once the backend has taken the message, so a refusal keeps the text.
    onSend(text)
      .then(
        () => setDraft((now) => (now.trim() === text ? "" : now)),
        () => undefined,
      )
      .finally(() => setSending(false));
  };
  return (
    <>
      {queued.length === 0 ? null : (
        <ul className="chat-queue" data-testid="chat-queue">
          {queued.map((n) => (
            <li key={n.id}>
              <span className="chat-queue-text" title={n.text}>
                {n.text}
              </span>
              <span className="chat-queue-why">Waiting: {QUEUE_WORDS[n.hold]}.</span>
              <button type="button" className="link" onClick={() => onWithdraw(n.id)}>
                Withdraw
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="conversation-input"
        onSubmit={(e) => {
          e.preventDefault();
          send();
        }}
      >
        <textarea
          value={draft}
          aria-label="Message to the agent"
          placeholder={busy ? "The agent is working. Your message waits until it is idle." : "Message the agent (Enter sends, Shift+Enter adds a line)"}
          rows={2}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
        />
        <div className="conversation-actions">
          <button type="submit" className="btn btn-primary" disabled={draft.trim() === "" || sending}>
            Send
          </button>
          {capabilities.stop && busy ? (
            <button type="button" className="btn" onClick={onStop}>
              Stop turn
            </button>
          ) : null}
          {capabilities.end ? (
            <button type="button" className="btn btn-danger" onClick={onEnd}>
              End agent
            </button>
          ) : null}
        </div>
      </form>
    </>
  );
}
