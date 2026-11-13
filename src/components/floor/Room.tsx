import { useContext, useEffect, useMemo, useRef, useState } from "react";
import { useReducedMotion } from "framer-motion";
import type { AgentMessage, DeskHandover, EditorList, Helper, ManagerFloor, Place, QaFloor, Session } from "../../types";
import { STATE_LABEL, TOOL_LABEL, checkoutLine, editorLabel, editorMenuItems, handoverOf, helpersLine, notesFor, plural, sessionMatches } from "../../state/selectors";
import { cardLine, tagWord } from "../../lib/handover";
import { poseForState } from "../../lib/animation";
import { headsLeft } from "../../lib/iso";
import type { PlanPoint } from "../../lib/iso";
import { DESK, ROOM, doorOf, floorSpotOf, managerSeat, podFor, podSeatOf, qaDeskSlot, roomDeskSlot, seatOf } from "../../lib/layout";
import type { Seat, Slot } from "../../lib/layout";
import { MANAGER_LOOK, lookFor } from "../../lib/looks";
import { Billboard, CameraContext, IsoBox } from "../../scene/Iso";
import type { StyleWithVars } from "../../scene/Iso";
import { Desk } from "../../scene/Desk";
import { materialsFor } from "../../scene/materials";
import { Duct, Tank, fuseBoxTop, toolboxTop } from "../../scene/PlantFittings";
import { Person, Walker } from "../../scene/Person";
import type { Handed } from "../../scene/Person";
import { BackWall, LeftWall, ManagerOffice, SignOutStand } from "./RoomObjects";
import type { RoomObjectsModel } from "./RoomObjects";
import type { Section } from "./sections";
import { Pets } from "./Pets";
import { EditorContext } from "../editor/EditorContext";
import { EditorMenu } from "../editor/EditorMenu";
import { Pod, PodScale } from "./Pod";
import { WorkerCard } from "./WorkerCard";
import { QaDesk } from "./QaDesk";

interface Walk {
  readonly id: string;
  /** handin: a finished helper takes its work to its agent's desk on the way out. */
  readonly kind: "note" | "arrive" | "leave" | "handin";
  readonly walkerId: string;
  /** Who receives what the walker carries. */
  readonly toId: string | null;
  readonly from: PlanPoint;
  /** A hand-in's stop at the agent's desk, between its own desk and the door. */
  readonly via: PlanPoint | null;
  readonly to: PlanPoint;
  /** 1 for an agent, smaller for a helper bound for a tiny desk. */
  readonly scale: number;
}

/** A session and the desk it sits at: the seated ones in grid order, then the QA reviewer in the corner. */
interface Placement {
  readonly session: Session;
  readonly desk: Slot;
}

interface RoomProps {
  /** The sessions at ordinary desks: the QA reviewer is not among them. */
  readonly sessions: readonly Session[];
  /** The floor's QA desk, when QA is on for it. */
  readonly qa: QaFloor | null;
  /** The QA reviewer, seated at the corner desk. */
  readonly reviewer: Session | undefined;
  /** The floor's manager, if it has an entry. Its session is never among `sessions`. */
  readonly manager: ManagerFloor | undefined;
  readonly depth: number;
  readonly query: string;
  readonly now: Date;
  readonly messages: readonly AgentMessage[];
  /** What each desk's handover has to say. */
  readonly handovers: readonly DeskHandover[];
  readonly objects: RoomObjectsModel;
  readonly hiring: boolean;
  readonly onOpenSession: (session: Session) => void;
  readonly onOpenSection: (section: Section) => void;
  readonly onOpenSettings: () => void;
  readonly onHire: () => void;
}

interface MenuState {
  readonly place: Place;
  readonly what: string;
  readonly caption: string;
  readonly x: number;
  readonly y: number;
}

/** Where a menu raised by `e` goes: the pointer, or the middle of the button when a keyboard raised it (0, 0). */
function menuPoint(e: React.MouseEvent<HTMLElement>): { readonly x: number; readonly y: number } {
  if (e.clientX !== 0 || e.clientY !== 0) return { x: e.clientX, y: e.clientY };
  const r = e.currentTarget.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

/** The hover card's last line, or null when nothing can be opened. */
function editorHint(list: EditorList | null): string | null {
  if (list === null) return "Right-click to open in an editor";
  const first = editorLabel(list, list.defaultEditor);
  return first === undefined ? null : `Right-click to open in ${first}`;
}

/** Every agent's seat, and each helper's while its pod is big enough to seat one. */
function seatsFor(placements: readonly Placement[]): Map<string, Seat> {
  const seats = new Map<string, Seat>();
  placements.forEach(({ session: s, desk }) => {
    seats.set(s.id, { at: seatOf(desk), scale: 1 });
    const pod = podFor(desk, s.helpers.length);
    pod.slots.forEach((slot, j) => {
      const helper = s.helpers[j];
      const seat = podSeatOf(pod, slot);
      if (helper !== undefined && seat !== null) seats.set(helper.id, seat);
    });
  });
  return seats;
}

/** Each helper's agent, by helper id. */
function agentsOf(placements: readonly Placement[]): Map<string, string> {
  return new Map(placements.flatMap(({ session: s }) => s.helpers.map((h) => [h.id, s.id] as const)));
}

const CARRIES: Record<Walk["kind"], Handed | null> = { note: "note", handin: "folder", arrive: null, leave: null };

/** Agents and their helpers, seated or not. */
function workerIds(placements: readonly Placement[]): string[] {
  return placements.flatMap(({ session: s }) => [s.id, ...s.helpers.map((h) => h.id)]);
}

/** The plant room's tank, in the front corner: its x, how far it stands from the front, its size. */
const TANK = { x: 8, back: 46, size: 30 } as const;

const ACTION_WORDS: Record<Session["control"], string> = {
  full: "Open its terminal.",
  "raise-window": "Bring its terminal window to the front.",
  "read-only": "Open its transcript.",
  structured: "Open its conversation.",
};

/** The open-plan floor: one desk per session, the hire desk, and the manager's objects. */
export function Room({ sessions, qa, reviewer, manager, depth, query, now, messages, handovers, objects, hiring, onOpenSession, onOpenSection, onOpenSettings, onHire }: RoomProps): React.ReactElement {
  const reduced = useReducedMotion() ?? false;
  const placements = useMemo<readonly Placement[]>(
    () => [...sessions.map((session, i) => ({ session, desk: roomDeskSlot(i) })), ...(reviewer === undefined ? [] : [{ session: reviewer, desk: qaDeskSlot(depth) }])],
    [sessions, reviewer, depth],
  );
  const managerId = manager?.status.kind === "hired" ? manager.status.sessionId : null;
  const seenMessages = useRef<Set<string>>(new Set(messages.map((m) => m.id)));
  const seenWorkers = useRef<Set<string>>(new Set(workerIds(placements)));
  // Where each agent and helper sat last time, so one that finishes can walk out from there.
  const lastSeats = useRef<Map<string, Seat>>(seatsFor(placements));
  const lastAgents = useRef<Map<string, string>>(agentsOf(placements));
  const [walks, setWalks] = useState<readonly Walk[]>([]);
  const [card, setCard] = useState<{ readonly sessionId: string; readonly x: number; readonly y: number } | null>(null);
  const showCard = (sessionId: string, el: HTMLElement): void => {
    const r = el.getBoundingClientRect();
    setCard({ sessionId, x: r.left + r.width / 2, y: r.top });
  };
  const hideCard = (): void => setCard(null);
  const editors = useContext(EditorContext);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const openMenu = (e: React.MouseEvent<HTMLElement>, place: Place, what: string, at: { readonly worktree: boolean; readonly branch: string | null }): void => {
    e.preventDefault();
    setCard(null);
    setMenu({ place, what, caption: `Opens ${checkoutLine(at)}`, ...menuPoint(e) });
  };
  const carded = card === null ? undefined : placements.find(({ session }) => session.id === card.sessionId)?.session;

  useEffect(() => {
    const seats = seatsFor(placements);
    // The manager walks its notes from its own seat, but takes no part in arrivals and departures.
    const noteSeats = managerId === null ? seats : new Map(seats).set(managerId, { at: managerSeat(), scale: 1 });
    const fresh: Walk[] = [];
    for (const m of messages) {
      if (seenMessages.current.has(m.id)) continue;
      seenMessages.current.add(m.id);
      const from = noteSeats.get(m.fromSessionId);
      const to = noteSeats.get(m.toSessionId);
      if (!reduced && from !== undefined && to !== undefined) {
        fresh.push({ id: m.id, kind: "note", walkerId: m.fromSessionId, toId: m.toSessionId, from: from.at, via: null, to: to.at, scale: 1 });
      }
    }
    for (const id of workerIds(placements)) {
      if (seenWorkers.current.has(id)) continue;
      seenWorkers.current.add(id);
      const seat = seats.get(id);
      if (!reduced && seat !== undefined) {
        fresh.push({ id: `arrive-${id}`, kind: "arrive", walkerId: id, toId: null, from: doorOf(depth), via: null, to: seat.at, scale: seat.scale });
      }
    }
    for (const [id, seat] of lastSeats.current) {
      if (reduced || seats.has(id)) continue;
      // A helper whose agent is still at its desk hands its work in, standing at
      // the front of that desk, on the way out.
      const agent = lastAgents.current.get(id);
      const at = placements.find(({ session }) => session.id === agent);
      fresh.push(
        agent !== undefined && at !== undefined
          ? { id: `leave-${id}`, kind: "handin", walkerId: id, toId: agent, from: seat.at, via: { x: seatOf(at.desk).x, y: at.desk.y + DESK.d + 6 }, to: doorOf(depth), scale: seat.scale }
          : { id: `leave-${id}`, kind: "leave", walkerId: id, toId: null, from: seat.at, via: null, to: doorOf(depth), scale: seat.scale },
      );
    }
    lastSeats.current = seats;
    lastAgents.current = agentsOf(placements);
    if (fresh.length === 0) return;
    setWalks((current) => {
      const busy = new Set(current.map((w) => w.walkerId));
      return [...current, ...fresh.filter((w) => !busy.has(w.walkerId))];
    });
  }, [messages, placements, depth, reduced, managerId]);

  const walking = new Set(walks.map((w) => w.walkerId));
  const managed = new Set(manager?.managed ?? []);
  const receiving = new Map(walks.flatMap((w) => (w.toId === null ? [] : [[w.toId, CARRIES[w.kind] ?? "note"] as const])));
  const hireSlot = roomDeskSlot(sessions.length);
  const toolbox = toolboxTop(TANK.x, depth - TANK.back, TANK.size);
  const fuse = fuseBoxTop(TANK.x, depth - TANK.back, TANK.size);

  return (
    <>
      <IsoBox x={0} y={0} w={ROOM.width} d={depth} h={ROOM.slab} top={materialsFor(objects.plant).floor} front="var(--concrete)" side="var(--concrete-shade)" />
      <BackWall model={objects} depth={depth} onOpen={onOpenSection} />
      {objects.plant ? (
        <>
          {/* Clear of the whiteboard and the hire desk: the duct over the pipes, the tank in the front corner. */}
          <Duct x={ROOM.width * 0.66} length={ROOM.width * 0.34} z={ROOM.slab + ROOM.wallHeight - 12} />
          <Tank x={TANK.x} y={depth - TANK.back} z={ROOM.slab} size={TANK.size} height={ROOM.wallHeight * 0.5} />
          <Billboard x={toolbox.x} y={toolbox.y} z={ROOM.slab + toolbox.h + 6}>
            <button type="button" className="memo-tag" data-testid="object-setup" aria-label="Toolbox: the building's setup. Open Setup." onClick={() => onOpenSection("setup")}>
              Toolbox
            </button>
          </Billboard>
          <Billboard x={fuse.x} y={fuse.y} z={ROOM.slab + fuse.h + 6}>
            <button type="button" className="memo-tag" data-testid="object-settings" aria-label="Fuse box: the app's settings. Open Settings." onClick={onOpenSettings}>
              Fuse box
            </button>
          </Billboard>
        </>
      ) : null}
      <LeftWall model={objects} depth={depth} onOpen={onOpenSection} />
      <ManagerOffice model={objects} depth={depth} onOpen={onOpenSection} away={managerId !== null && walking.has(managerId)} />
      <SignOutStand model={objects} depth={depth} onOpen={onOpenSection} />

      {qa === null ? null : <QaDesk qa={qa} depth={depth} occupied={reviewer !== undefined} now={now} onOpen={() => onOpenSection("info")} />}

      {placements.map(({ session: s, desk }) => {
        const seat = s.state === "error" ? floorSpotOf(desk, depth - 4) : seatOf(desk);
        const dim = !sessionMatches(s, query);
        const waiting = notesFor(manager, s.id);
        const handed = handoverOf(handovers, s.id);
        return (
          <div key={s.id} className={dim ? "group is-dim" : "group"}>
            <Desk x={desk.x} y={desk.y} z={ROOM.slab} led={s.state} plant={s.worktree} />
            <Pod
              desk={desk}
              z={ROOM.slab}
              helpers={s.helpers}
              walking={walking}
              onHelperMenu={editors === null ? undefined : (h: Helper, e) => openMenu(e, { kind: "helper", sessionId: s.id, helperId: h.id }, `a helper of \u201c${s.title}\u201d`, h)}
            />
            <Pets desk={desk} z={ROOM.slab} pets={s.pets} />
            {waiting.length === 0 ? null : (
              <Billboard x={desk.x + DESK.w - 6} y={desk.y + DESK.d} z={ROOM.slab + DESK.h + 2}>
                <span className="desk-note" data-testid="desk-note">
                  {waiting.length}
                </span>
              </Billboard>
            )}
            {walking.has(s.id) ? null : (
              <Billboard x={seat.x} y={seat.y} z={ROOM.slab}>
                <button
                  type="button"
                  className={s.state === "error" ? "worker is-down" : "worker"}
                  data-testid="worker"
                  data-session={s.id}
                  data-state={s.state}
                  aria-label={[s.title, TOOL_LABEL[s.tool], STATE_LABEL[s.state], helpersLine(s), (handed === undefined ? null : cardLine(handed.state, now)), s.worktree ? (s.branch === null ? "in a worktree" : `in a worktree on branch ${s.branch}`) : null, managed.has(s.id) ? "managed by the floor's manager" : null, waiting.length === 0 ? null : `${plural(waiting.length, "note", "notes")} from the manager waiting`].filter((p) => p !== null).join(", ") + `. ${ACTION_WORDS[s.control]}`}
                  aria-keyshortcuts={editors === null ? undefined : "Meta+Shift+O"}
                  onClick={() => onOpenSession(s)}
                  onContextMenu={editors === null ? undefined : (e) => openMenu(e, { kind: "session", sessionId: s.id }, `\u201c${s.title}\u201d`, s)}
                  onMouseEnter={(e) => showCard(s.id, e.currentTarget)}
                  onMouseLeave={hideCard}
                  onFocus={(e) => showCard(s.id, e.currentTarget)}
                  onBlur={hideCard}
                >
                  <Person look={lookFor(s.id)} pose={poseForState(s.state)} receiving={receiving.get(s.id) ?? null} />
                  <span className={`name-tag state-${s.state}`} aria-hidden>
                    {s.title}
                  </span>
                  {managed.has(s.id) ? (
                    <span className="managed-mark" aria-hidden>
                      managed
                    </span>
                  ) : null}
                  {handed === undefined ? null : (
                    <span className="managed-mark handover-mark" aria-hidden>
                      {tagWord(handed.state)}
                    </span>
                  )}
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

      {carded === undefined || card === null ? null : (
        <WorkerCard
          session={carded}
          x={card.x}
          y={card.y}
          now={now}
          editorHint={editors === null ? null : editorHint(editors.list)}
          managed={managed.has(carded.id)}
          handover={handoverOf(handovers, carded.id)?.state}
          waiting={notesFor(manager, carded.id)}
        />
      )}

      {menu === null || editors === null ? null : (
        <EditorMenu
          at={{ x: menu.x, y: menu.y }}
          caption={menu.caption}
          items={editorMenuItems(editors.list)}
          onPick={(editor) => {
            editors.open(menu.place, editor, menu.what);
            setMenu(null);
          }}
          onClose={() => setMenu(null)}
        />
      )}

      {walks.map((w) => (
        <WalkView key={w.id} walk={w} depth={depth} managerId={managerId} onDone={(id) => setWalks((cur) => cur.filter((x) => x.id !== id))} />
      ))}
    </>
  );
}

function WalkView({ walk, depth, managerId, onDone }: { readonly walk: Walk; readonly depth: number; readonly managerId: string | null; readonly onDone: (id: string) => void }): React.ReactElement {
  const cam = useContext(CameraContext);
  const first = walk.via ?? walk.to;
  const left = headsLeft(cam, ROOM.width, depth, walk.from, first);
  const walker = <Walker look={walk.walkerId === managerId ? MANAGER_LOOK : lookFor(walk.walkerId)} carrying={CARRIES[walk.kind]} />;
  const style: StyleWithVars = {
    "--fx": `${walk.from.x}px`,
    "--fy": `${walk.from.y}px`,
    "--vx": `${first.x}px`,
    "--vy": `${first.y}px`,
    // Which way it faces on each leg: towards the agent's desk, then towards the door.
    "--face-1": left ? "-1" : "1",
    "--face-2": headsLeft(cam, ROOM.width, depth, first, walk.to) ? "-1" : "1",
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
        {walk.scale === 1 ? walker : <PodScale scale={walk.scale}>{walker}</PodScale>}
      </Billboard>
    </div>
  );
}
