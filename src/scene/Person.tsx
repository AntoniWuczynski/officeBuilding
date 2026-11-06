import type { Pose } from "../lib/animation";
import type { Look } from "../lib/looks";
import { rig } from "./cast/rig";
import type { Part, Placed } from "./cast/rig";
import type { StyleWithVars } from "./Iso";

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

/** What one worker hands another: a note is a message, a manila folder a finished subagent's work. */
export type Handed = "note" | "folder";

/** Something just received from a colleague, popping up briefly. */
function ReceivedBubble({ handed }: { readonly handed: Handed }): React.ReactElement {
  return (
    <span className="bubble bubble-note" aria-hidden>
      <svg viewBox="0 0 12 12" width="12" height="12">
        {handed === "note" ? (
          <>
            <path d="M2 1.5h6l2 2v7H2z" fill="#FFFDF8" stroke="#1C1F24" strokeWidth="1" />
            <path d="M4 5h4M4 7h4" stroke="#1C1F24" strokeWidth=".8" />
          </>
        ) : (
          <path d="M1.5 3.5h3l1 1h5v6h-9z" fill="#E3C27A" stroke="#1C1F24" strokeWidth="1" />
        )}
      </svg>
    </span>
  );
}

/** One cut-out piece: its picture, and the pieces hinged to it. */
function Piece({ at, part, className, children }: { readonly at: Placed; readonly part?: Part; readonly className: string; readonly children?: React.ReactNode }): React.ReactElement {
  const style: StyleWithVars = { left: at.left, top: at.top, width: at.w, height: at.h, transformOrigin: `${at.ox}px ${at.oy}px` };
  return (
    <span className={`pp ${className}`} style={style}>
      {part === undefined ? null : <img src={part.src} alt="" draggable={false} />}
      {children}
    </span>
  );
}

/**
 * A South Park-style cut-out of a realistic person (art/cast): flat pieces that
 * snap between poses. Seated at a desk, or standing (walking, or flat on the
 * floor when stuck), with legs.
 */
function Puppet({ look, standing, carrying = null }: { readonly look: Look; readonly standing: boolean; readonly carrying?: Handed | null }): React.ReactElement {
  const p = look.parts;
  const r = rig(p, standing);
  const [armA, armB] = r.upperArm;
  const [foreA, foreB] = r.forearm;
  const [legA, legB] = r.leg;
  return (
    <span className="puppet" aria-hidden>
      <span className="pp-rig">
        <Piece at={r.torso} className="pp-torso">
          {standing ? null : <Piece at={r.chair} part={p.chair} className="pp-chair" />}
          <Piece at={r.pelvis} className="pp-pelvis">
            {standing ? (
              <>
                <Piece at={legA} part={p["leg-r"]} className="pp-leg pp-leg-a" />
                <Piece at={legB} part={p["leg-l"]} className="pp-leg pp-leg-b" />
              </>
            ) : null}
            <img src={p.pelvis.src} alt="" draggable={false} />
          </Piece>
          <Piece at={r.head} part={p.head} className="pp-head">
            <img className="pp-blink" src={p["head-closed"].src} alt="" draggable={false} />
          </Piece>
          {/* Upper arms tuck behind the shirt; a second, bare copy of each arm carries its forearm in front. */}
          <Piece at={armA} part={p["upper-arm-r"]} className="pp-arm pp-arm-a" />
          <Piece at={armB} part={p["upper-arm-l"]} className="pp-arm pp-arm-b" />
          <img src={p.torso.src} alt="" draggable={false} />
          <Piece at={armA} className="pp-arm pp-arm-a">
            <Piece at={foreA} part={p["forearm-r"]} className="pp-fore pp-fore-a" />
          </Piece>
          <Piece at={armB} className="pp-arm pp-arm-b">
            <Piece at={foreB} part={p["forearm-l"]} className="pp-fore pp-fore-b">
              {carrying === null ? null : <i className={`pp-${carrying}`} />}
            </Piece>
          </Piece>
        </Piece>
      </span>
    </span>
  );
}

interface PersonProps {
  readonly look: Look;
  readonly pose: Pose;
  /** Show the "just got a note (or folder)" reaction. */
  readonly receiving?: Handed | null;
}

/** An employee at their desk, or flat on the floor when stuck. The pose encodes the session state (DESIGN.md, motion). */
export function Person({ look, pose, receiving = null }: PersonProps): React.ReactElement {
  return (
    <span className={`person pose-${pose}`} data-pose={pose}>
      <Bubble pose={pose} />
      {receiving === null ? null : <ReceivedBubble handed={receiving} />}
      <Puppet look={look} standing={pose === "stuck"} />
    </span>
  );
}

/** A standing, walking employee, optionally carrying a note or a folder. */
export function Walker({ look, carrying }: { readonly look: Look; readonly carrying: Handed | null }): React.ReactElement {
  return (
    <span className="walker">
      <span className="w-flip">
        <Puppet look={look} standing carrying={carrying} />
      </span>
    </span>
  );
}
