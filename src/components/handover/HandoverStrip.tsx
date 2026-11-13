import { useContext, useState } from "react";
import type { DeskHandoverState } from "../../types";
import { message } from "../../lib/errors";
import { confirmSentence, stripLine } from "../../lib/handover";
import { TOOL_LABEL } from "../../state/selectors";
import { HandoverContext } from "./HandoverContext";

/** What a desk's handover has to say, in the panel: one line, and for a used-up desk the button that hands it over. */
export function HandoverStrip({ sessionId, state }: { readonly sessionId: string; readonly state: DeskHandoverState }): React.ReactElement {
  const api = useContext(HandoverContext);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const line = stripLine(state, new Date());

  const handOver = (): void => {
    if (api === null) return;
    setBusy(true);
    setError(null);
    api.handOverDesk(sessionId).then(
      () => {
        setBusy(false);
        setConfirming(false);
      },
      (e: unknown) => {
        setBusy(false);
        setError(message(e));
      },
    );
  };

  return (
    <>
      <div className="route-strip handover-strip" data-testid="handover-strip">
        <span className="route-lamp" data-tone={line.tone} aria-hidden="true" />
        <span>{line.text}</span>
        {state.kind === "offered" && api !== null ? (
          <button type="button" className="btn dark-btn" data-testid="handover-button" onClick={() => setConfirming(true)}>
            Hand over to {TOOL_LABEL[state.to]}
          </button>
        ) : null}
      </div>
      {state.kind === "offered" && confirming ? (
        <div className="route-details handover-confirm" role="group" aria-label="Hand over" data-testid="handover-confirm">
          <p>{confirmSentence(state)}</p>
          <span className="route-buttons">
            <button type="button" className="btn dark-btn" disabled={busy} data-testid="handover-confirm-yes" onClick={handOver}>
              {busy ? "Handing over" : "End it and hand over"}
            </button>
            <button type="button" className="btn dark-btn" disabled={busy} data-testid="handover-confirm-no" onClick={() => setConfirming(false)}>
              Cancel
            </button>
          </span>
          {error === null ? null : <p role="alert">Could not hand over: {error}.</p>}
        </div>
      ) : null}
    </>
  );
}
