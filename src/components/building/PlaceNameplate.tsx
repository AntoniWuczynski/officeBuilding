import type { FloorLight, Verdict } from "../../types";
import type { Counts } from "../../state/selectors";
import { STAMP_WORDS } from "../../state/selectors";
import { lightWords } from "../../lib/animation";
import { PLACE } from "../../state/placeWords";
import type { ViewMode } from "../../state/useViewMode";
import { DemolishIcon, SinkIcon } from "../icons";
import { QaStamp } from "../qa/QaStamp";
import "./place.css";

/** What differs between the two skins: the words, the icon and the hooks the tests and scripts find them by. */
const SKIN: Readonly<Record<ViewMode, { readonly kind: string; readonly go: string; readonly removeTitle: string; readonly plateId: string; readonly removeId: string | undefined; readonly icon: React.ReactElement }>> = {
  office: { kind: "Plant room", go: "Go to floor.", removeTitle: "Demolish this floor", plateId: "floor-nameplate", removeId: "demolish-floor", icon: <DemolishIcon /> },
  islands: { kind: "Lighthouse", go: "Show island.", removeTitle: "Sink this island", plateId: "island-nameplate", removeId: undefined, icon: <SinkIcon /> },
};

interface Props {
  readonly mode: ViewMode;
  readonly projectId: string;
  readonly name: string;
  readonly light: FloorLight;
  readonly counts: Counts;
  /** Questions waiting on the human here. */
  readonly pending: number;
  /** Agents past the ones the place has room to draw. */
  readonly overflow: number;
  readonly plant: boolean;
  readonly stamp: Verdict | null;
  /** The place is going away: it cannot be removed twice. */
  readonly leaving: boolean;
  /** Whether this place's panel is open, and its id. */
  readonly expanded: boolean;
  readonly panelId: string;
  readonly onShow: () => void;
  readonly onRemove: () => void;
}

/** One spoken line for a place: its name, how busy it is, what waits on you, and what the plate does. */
function spoken({ mode, name, light, counts, pending, overflow, stamp }: Pick<Props, "mode" | "name" | "light" | "counts" | "pending" | "overflow" | "stamp">): string {
  return `${name}: ${counts.active} of ${counts.total} active, ${lightWords(light)}${pending > 0 ? `, ${pending} waiting on you` : ""}${overflow > 0 ? `, ${overflow} more not shown` : ""}${stamp === null ? "" : `, ${STAMP_WORDS[stamp].spoken}`}. ${SKIN[mode].go}`;
}

/** A place's plate and its remove button, in either view: lamp, name, count (with the agents that do not fit), QA stamp, waiting count. */
export function PlaceNameplate(p: Props): React.ReactElement {
  const skin = SKIN[p.mode];
  return (
    <>
      <button
        type="button"
        className="nameplate"
        data-testid={skin.plateId}
        data-project={p.projectId}
        aria-label={spoken(p)}
        aria-expanded={p.expanded}
        aria-controls={p.expanded ? p.panelId : undefined}
        onClick={(e) => {
          // On a storey the click would otherwise also walk into the floor.
          e.stopPropagation();
          p.onShow();
        }}
      >
        <i className="lamp" aria-hidden />
        <span className="nameplate-name">{p.name}</span>
        {p.plant ? <span className="nameplate-kind">{skin.kind}</span> : null}
        <span className="nameplate-count">
          {p.counts.active}/{p.counts.total}
        </span>
        {p.overflow > 0 ? <span className="nameplate-more">+{p.overflow}</span> : null}
        {p.stamp === null ? null : <QaStamp verdict={p.stamp} />}
        {p.pending > 0 ? (
          <span className="nameplate-pending" aria-label={`${p.pending} waiting on you`}>
            {p.pending}
          </span>
        ) : null}
      </button>
      {p.plant ? null : (
        <button
          type="button"
          className="demolish-btn"
          data-testid={skin.removeId}
          aria-label={PLACE[p.mode].removeLabel(p.name)}
          title={skin.removeTitle}
          disabled={p.leaving}
          onClick={(e) => {
            e.stopPropagation();
            p.onRemove();
          }}
        >
          {skin.icon}
        </button>
      )}
    </>
  );
}
