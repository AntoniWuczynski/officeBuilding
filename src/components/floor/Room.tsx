import { useContext, useEffect, useRef, useState } from "react";
import { useReducedMotion } from "framer-motion";
import type { AgentMessage, Session } from "../../types";
import { STATE_LABEL, TOOL_LABEL, helpersLine, sessionMatches } from "../../state/selectors";
import { poseForState } from "../../lib/animation";
import { headsLeft } from "../../lib/iso";
import type { PlanPoint } from "../../lib/iso";
import { DESK, ROOM, doorOf, floorSpotOf, roomDeskSlot, seatOf } from "../../lib/layout";
import { lookFor } from "../../lib/looks";
import { Billboard, CameraContext, IsoBox } from "../../scene/Iso";
import type { StyleWithVars } from "../../scene/Iso";
import { Desk } from "../../scene/Desk";
import { Person, Walker } from "../../scene/Person";
import { BackWall, LeftWall, ManagerOffice } from "./RoomObjects";
import type { RoomObjectsModel } from "./RoomObjects";
import type { Section } from "./sections";
import { Pod } from "./Pod";
import { WorkerCard } from "./WorkerCard";

interface Walk {
  readonly id: string;
  readonly kind: "note" | "arrive";
  readonly walkerId: string;
  readonly toId: string | null;
  readonly from: PlanPoint;
  readonly to: PlanPoint;
}

interface RoomProps {
  readonly sessions: readonly Session[];
  readonly depth: number;
  readonly query: string;
  readonly now: Date;
  readonly messages: readonly AgentMessage[];
  readonly objects: RoomObjectsModel;
  readonly hiring: boolean;
  readonly onOpenSession: (session: Session) => void;
  readonly onOpenSection: (section: Section) => void;
  readonly onHire: () => void;
}

function seatsFor(sessions: readonly Session[]): Map<string, PlanPoint> {
  return new Map(sessions.map((s, i) => [s.id, seatOf(roomDeskSlot(i))]));
}

const ACTION_WORDS: Record<Session["control"], string> = {
  full: "Open its terminal.",
  "raise-window": "Bring its terminal window to the front.",
  "read-only": "Open its transcript.",
};

/** The open-plan floor: one desk per session, the hire desk, and the manager's objects. */
export function Room({ sessions, depth, query, now, messages, objects, hiring, onOpenSession, onOpenSection, onHire }: RoomProps): React.ReactElement {
  const reduced = useReducedMotion() ?? false;
  const seenMessages = useRef<Set<string>>(new Set(messages.map((m) => m.id)));
  const seenSessions = useRef<Set<string>>(new Set(sessions.map((s) => s.id)));
  const [walks, setWalks] = useState<readonly Walk[]>([]);
  const [card, setCard] = useState<{ readonly sessionId: string; readonly x: number; readonly y: number } | null>(null);
  const showCard = (sessionId: string, el: HTMLElement): void => {
    const r = el.getBoundingClientRect();
    setCard({ sessionId, x: r.left + r.width / 2, y: r.top });
  };
  const hideCard = (): void => setCard(null);
  const carded = card === null ? undefined : sessions.find((s) => s.id === card.sessionId);

  useEffect(() => {
    const seats = seatsFor(sessions);
    const fresh: Walk[] = [];
    for (const m of messages) {
      if (seenMessages.current.has(m.id)) continue;
      seenMessages.current.add(m.id);
      const from = seats.get(m.fromSessionId);
      const to = seats.get(m.toSessionId);
      if (!reduced && from !== undefined && to !== undefined) {
        fresh.push({ id: m.id, kind: "note", walkerId: m.fromSessionId, toId: m.toSessionId, from, to });
      }
    }
    for (const s of sessions) {
      if (seenSessions.current.has(s.id)) continue;
      seenSessions.current.add(s.id);
      const seat = seats.get(s.id);
      if (!reduced && seat !== undefined) {
        fresh.push({ id: `arrive-${s.id}`, kind: "arrive", walkerId: s.id, toId: null, from: doorOf(depth), to: seat });
      }
    }
    if (fresh.length === 0) return;
    setWalks((current) => {
      const busy = new Set(current.map((w) => w.walkerId));
      return [...current, ...fresh.filter((w) => !busy.has(w.walkerId))];
    });
  }, [messages, sessions, depth, reduced]);

  const walking = new Set(walks.map((w) => w.walkerId));
  const receiving = new Set(walks.flatMap((w) => (w.toId === null ? [] : [w.toId])));
  const hireSlot = roomDeskSlot(sessions.length);

  return (
    <>
      <IsoBox x={0} y={0} w={ROOM.width} d={depth} h={ROOM.slab} top="var(--boards)" front="var(--concrete)" side="var(--concrete-shade)" />
      <BackWall model={objects} depth={depth} onOpen={onOpenSection} />
      <LeftWall model={objects} depth={depth} onOpen={onOpenSection} />
      <ManagerOffice model={objects} depth={depth} onOpen={onOpenSection} />

      {sessions.map((s, i) => {
        const desk = roomDeskSlot(i);
        const seat = s.state === "error" ? floorSpotOf(desk, depth - 4) : seatOf(desk);
        const dim = !sessionMatches(s, query);
        return (
          <div key={s.id} className={dim ? "group is-dim" : "group"}>
            <Desk x={desk.x} y={desk.y} z={ROOM.slab} led={s.state} />
            <Pod desk={desk} z={ROOM.slab} helpers={s.helpers} />
            {walking.has(s.id) ? null : (
              <Billboard x={seat.x} y={seat.y} z={ROOM.slab}>
                <button
                  type="button"
                  className={s.state === "error" ? "worker is-down" : "worker"}
                  data-testid="worker"
                  data-session={s.id}
                  data-state={s.state}
                  aria-label={[s.title, TOOL_LABEL[s.tool], STATE_LABEL[s.state], helpersLine(s)].filter((p) => p !== null).join(", ") + `. ${ACTION_WORDS[s.control]}`}
                  onClick={() => onOpenSession(s)}
                  onMouseEnter={(e) => showCard(s.id, e.currentTarget)}
                  onMouseLeave={hideCard}
                  onFocus={(e) => showCard(s.id, e.currentTarget)}
                  onBlur={hideCard}
                >
                  <Person look={lookFor(s.id)} pose={poseForState(s.state)} receiving={receiving.has(s.id)} />
                  <span className={`name-tag state-${s.state}`} aria-hidden>
                    {s.title}
                  </span>
                </button>
              </Billboard>
            )}
          </div>
        );
      })}

      <IsoBox x={hireSlot.x} y={hireSlot.y} z={ROOM.slab} w={DESK.w} d={DESK.d} h={DESK.h} top="var(--paper)" front="var(--rule)" side="var(--concrete)" className="hire-desk" />
      <Billboard x={hireSlot.x + 18} y={hireSlot.y} z={ROOM.slab + DESK.h}>
        <button type="button" className="hire-sign" data-testid="hire-desk" disabled={hiring} onClick={onHire}>
          <span aria-hidden>+</span> {hiring ? "Hiring" : "Hire an agent"}
        </button>
      </Billboard>

      {carded === undefined || card === null ? null : <WorkerCard session={carded} x={card.x} y={card.y} now={now} />}

      {walks.map((w) => (
        <WalkView key={w.id} walk={w} depth={depth} onDone={(id) => setWalks((cur) => cur.filter((x) => x.id !== id))} />
      ))}
    </>
  );
}

function WalkView({ walk, depth, onDone }: { readonly walk: Walk; readonly depth: number; readonly onDone: (id: string) => void }): React.ReactElement {
  const cam = useContext(CameraContext);
  const left = headsLeft(cam, ROOM.width, depth, walk.from, walk.to);
  const style: StyleWithVars = {
    "--fx": `${walk.from.x}px`,
    "--fy": `${walk.from.y}px`,
    "--tx": `${walk.to.x}px`,
    "--ty": `${walk.to.y}px`,
    "--z": `${ROOM.slab}px`,
  };
  return (
    <div
      className={`walk walk-${walk.kind}${left ? " heads-left" : ""}`}
      style={style}
      data-testid="walk"
      onAnimationEnd={(e) => {
        if (e.target === e.currentTarget) onDone(walk.id);
      }}
    >
      <Billboard x={0} y={0}>
        <Walker look={lookFor(walk.walkerId)} carrying={walk.kind === "note"} />
      </Billboard>
    </div>
  );
}
