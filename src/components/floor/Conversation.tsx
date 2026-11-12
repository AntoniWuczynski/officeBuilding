import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { DiscoveryApi } from "../../api/DiscoveryApi";
import type { Approval, ChatCapabilities, ConversationBacklog, ConversationEntry, ConversationUpdate, QueuedNote, SessionState } from "../../types";
import { message } from "../../lib/errors";
import { ApprovalCard } from "../chat/ApprovalCard";
import { Composer } from "../chat/Composer";
import { EntryLine } from "../chat/EntryLine";

export type ConversationApi = Pick<
  DiscoveryApi,
  "structuredBacklog" | "subscribeStructured" | "structuredPrompt" | "structuredAnswer" | "structuredCancel" | "structuredStop" | "withdrawMessage"
>;

/** The structured-desk operations, provided by App beside the terminal ones. */
export const ConversationApiContext = createContext<ConversationApi | null>(null);

interface View {
  readonly transcript: readonly ConversationEntry[];
  readonly state: SessionState;
  readonly approval: Approval | null;
  readonly ended: string | null;
  readonly capabilities: ChatCapabilities;
  readonly earlier: boolean;
}

/** The most entries the panel holds; older ones are dropped as new ones arrive. */
export const PANEL_MAX = 400;

/** Add an entry, or replace the one with the same id (streamed text grows, a tool call finishes). */
function upsert(list: readonly ConversationEntry[], entry: ConversationEntry): readonly ConversationEntry[] {
  const at = list.findIndex((e) => e.id === entry.id);
  return at === -1 ? [...list, entry] : list.map((e, i) => (i === at ? entry : e));
}

function apply(view: View, u: ConversationUpdate): View {
  const base = u.reset ? [] : view.transcript;
  const grown = u.entry === null ? base : upsert(base, u.entry);
  const over = grown.length > PANEL_MAX;
  return {
    transcript: over ? grown.slice(grown.length - PANEL_MAX) : grown,
    state: u.state,
    approval: u.approval,
    ended: u.ended,
    capabilities: view.capabilities,
    earlier: view.earlier || over,
  };
}

function open(b: ConversationBacklog): View {
  const { transcript, state, approval, ended, capabilities, earlier } = b;
  return { transcript, state, approval, ended, capabilities, earlier };
}

/**
 * A chat desk's conversation, in the docked panel: the transcript, an open
 * approval, and what the desk's capabilities allow below it. It knows no provider:
 * the backlog says what to offer. The agent keeps running when the panel closes.
 */
export function Conversation({
  sessionId,
  queued = [],
  onFlip,
}: {
  readonly sessionId: string;
  readonly queued?: readonly QueuedNote[] | undefined;
  readonly onFlip?: (() => void) | undefined;
}): React.ReactElement {
  const api = useContext(ConversationApiContext);
  if (api === null) throw new Error("Conversation needs a ConversationApiContext provider");
  const [view, setView] = useState<View | null>(null);
  const [error, setError] = useState<string | null>(null);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Updates that arrive before the backlog are replayed on top of it.
    const early: ConversationUpdate[] = [];
    let loaded = false;
    const stop = api.subscribeStructured(sessionId, (u) => {
      if (loaded) setView((v) => (v === null ? v : apply(v, u)));
      else early.push(u);
    });
    api.structuredBacklog(sessionId).then(
      (b) => {
        loaded = true;
        setView(early.reduce(apply, open(b)));
      },
      (e: unknown) => setError(message(e)),
    );
    return stop;
  }, [api, sessionId]);

  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [view?.transcript.length, view?.approval]);

  // Shows a refusal and passes it on, so a caller that must keep something on refusal can.
  const attempt = (call: Promise<void>): Promise<void> => {
    setError(null);
    return call.catch((e: unknown) => {
      setError(message(e));
      throw e;
    });
  };
  const run = (call: Promise<void>): void => {
    attempt(call).catch(() => undefined);
  };

  if (view === null) {
    return <p className="terminal-note">{error === null ? "Opening the conversation…" : `The conversation could not be opened: ${error}.`}</p>;
  }
  const busy = view.state === "thinking" || view.state === "running";
  // A tool waiting on an approval is not stopped: only a desk at rest has stopped its tools.
  const atRest = view.state === "idle" || view.state === "done" || view.state === "error";
  const { capabilities } = view;
  return (
    <div className="conversation" data-testid="conversation">
      <ol className="conversation-log">
        {view.earlier ? <li className="chat-earlier">Earlier messages are not shown here.</li> : null}
        {view.transcript.map((e) => (
          <EntryLine key={e.id} entry={e} busy={!atRest} />
        ))}
        {view.transcript.length === 0 && view.ended === null ? <li className="conversation-hint">Nothing has been said yet.</li> : null}
      </ol>
      {view.approval === null ? null : (
        <ApprovalCard
          key={view.approval.id}
          approval={view.approval}
          mode={capabilities.approvals}
          onFlip={onFlip}
          onAnswer={(choiceId, values) => {
            const approval = view.approval;
            if (approval !== null) run(api.structuredAnswer(sessionId, approval.id, choiceId, values));
          }}
        />
      )}
      <div ref={end} />
      {error === null ? null : (
        <p className="terminal-note terminal-status" role="alert">
          {error}.
        </p>
      )}
      {view.ended !== null ? (
        <p className="terminal-note terminal-status" role="status">
          {view.ended} The conversation stays above.
        </p>
      ) : !capabilities.compose ? (
        <p className="terminal-note chat-readonly">This conversation is read-only here.</p>
      ) : (
        <Composer
          capabilities={capabilities}
          busy={busy}
          queued={queued}
          onSend={(text) => attempt(api.structuredPrompt(sessionId, text))}
          onWithdraw={(id) => run(api.withdrawMessage(id))}
          onStop={() => run(api.structuredCancel(sessionId))}
          onEnd={() => run(api.structuredStop(sessionId))}
        />
      )}
    </div>
  );
}
