import type { Pose } from "../lib/animation";
import type { Look } from "../lib/looks";

function Bubble({ pose }: { readonly pose: Pose }): React.ReactElement | null {
  switch (pose) {
    case "hand-up":
      return <span className="bubble bubble-ask" aria-hidden>?</span>;
    case "stuck":
      return <span className="bubble bubble-stuck" aria-hidden>!</span>;
    case "thinking":
      return (
        <span className="bubble bubble-think" aria-hidden>
          <i />
          <i />
          <i />
        </span>
      );
    default:
      return null;
  }
}

/** A note received from a colleague: a folded paper that pops up briefly. */
function NoteBubble(): React.ReactElement {
  return (
    <span className="bubble bubble-note" aria-hidden>
      <svg viewBox="0 0 12 12" width="12" height="12">
        <path d="M2 1.5h6l2 2v7H2z" fill="#FFFDF8" stroke="#1C1F24" strokeWidth="1" />
        <path d="M4 5h4M4 7h4" stroke="#1C1F24" strokeWidth=".8" />
      </svg>
    </span>
  );
}

interface PersonProps {
  readonly look: Look;
  readonly pose: Pose;
  /** Show the "just got a note" reaction. */
  readonly receiving?: boolean;
}

/** A stuck agent (error): flat on the floor, X for eyes (owner request, 2026-09-29). */
function Down({ look }: { readonly look: Look }): React.ReactElement {
  return (
    <svg viewBox="0 0 34 14" width="34" height="14" aria-hidden>
      <rect x="23" y="5.5" width="11" height="3.2" rx="1.6" fill="#2F3440" />
      <rect x="23" y="9" width="10" height="3.2" rx="1.6" fill="#2F3440" />
      <rect x="9" y="3.5" width="15" height="10" rx="3.5" fill={look.shirt} />
      <rect x="11" y="0.6" width="9" height="3.2" rx="1.6" fill={look.shirt} transform="rotate(-18 15 2)" />
      <circle cx="5.2" cy="8.4" r="4.4" fill={look.skin} />
      <path d="M1 7.2a4.4 4.4 0 0 1 3-3.1" stroke={look.hair} strokeWidth="2.2" fill="none" />
      <path d="M3.4 6.8l1.8 1.8M5.2 6.8L3.4 8.6M6.2 8.6l1.8 1.8M8 8.6L6.2 10.4" stroke="#1C1F24" strokeWidth=".8" />
    </svg>
  );
}

/** A seated employee. The pose loops encode the session state (DESIGN.md, motion). */
export function Person({ look, pose, receiving = false }: PersonProps): React.ReactElement {
  if (pose === "stuck") {
    return (
      <span className="person pose-stuck" data-pose={pose}>
        <Bubble pose={pose} />
        {receiving ? <NoteBubble /> : null}
        <Down look={look} />
      </span>
    );
  }
  const hand =
    pose === "hand-up" ? (
      <g className="p-wave">
        <rect x="14.6" y="1.5" width="3.2" height="11" rx="1.6" fill={look.shirt} />
        <circle cx="16.2" cy="2.4" r="1.9" fill={look.skin} />
      </g>
    ) : (
      <rect className="p-arm p-arm-r" x="14.6" y="12" width="3.2" height="9" rx="1.6" fill={look.shirt} />
    );
  return (
    <span className={`person pose-${pose}`} data-pose={pose}>
      <Bubble pose={pose} />
      {receiving ? <NoteBubble /> : null}
      <svg viewBox="0 0 20 30" width="20" height="30" aria-hidden>
        <g className="p-body">
          <rect className="p-arm p-arm-l" x="2.2" y="12" width="3.2" height="9" rx="1.6" fill={look.shirt} />
          {hand}
          <rect x="5" y="11" width="10" height="14" rx="3.5" fill={look.shirt} />
          <circle cx="10" cy="6.5" r="4.4" fill={look.skin} />
          <path d="M5.6 6.4a4.4 4.4 0 0 1 8.8 0q-4.4-2.4-8.8 0z" fill={look.hair} />
        </g>
      </svg>
    </span>
  );
}

/** A standing, walking employee, optionally carrying a note. */
export function Walker({ look, carrying }: { readonly look: Look; readonly carrying: boolean }): React.ReactElement {
  return (
    <span className="walker">
      <svg viewBox="0 0 22 38" width="22" height="38" aria-hidden>
        <g className="w-flip">
          <rect className="w-leg w-leg-a" x="6" y="24" width="3.4" height="12" rx="1.6" fill="#2F3440" />
          <rect className="w-leg w-leg-b" x="11" y="24" width="3.4" height="12" rx="1.6" fill="#2F3440" />
          <rect x="5" y="11" width="11" height="15" rx="3.5" fill={look.shirt} />
          {carrying ? (
            <rect x="14.5" y="13" width="7" height="9" fill="#FFFDF8" stroke="#8C8C8C" strokeWidth=".6" transform="rotate(10 18 17)" />
          ) : null}
          <circle cx="10.5" cy="6.5" r="4.4" fill={look.skin} />
          <path d="M6.1 6.4a4.4 4.4 0 0 1 8.8 0q-4.4-2.4-8.8 0z" fill={look.hair} />
        </g>
      </svg>
    </span>
  );
}
