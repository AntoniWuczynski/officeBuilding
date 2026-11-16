import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useReducedMotion } from "framer-motion";
import type { QuestionAnswer } from "../../api/DiscoveryApi";
import type { OfficeSnapshot, Project, Question, Session, ToolUsage } from "../../types";
import type { ViewMode } from "../../state/useViewMode";
import { PLACE } from "../../state/placeWords";
import {
  floorLight,
  matchesQuery,
  qaFor,
  questionsForProject,
  sessionById,
  sessionCounts,
  sessionsForProject,
} from "../../state/selectors";
import { BUILDING_CAMERA } from "../../lib/iso";
import { STOREY, storeyWidth } from "../../lib/layout";
import { DEFAULT_BUILDING_NAME, useBuildingName } from "../../state/useBuildingName";
import { usePresence } from "../../state/usePresence";
import { Stage } from "../../scene/Stage";
import { AddFloorDialog } from "./AddFloorDialog";
import { DemolishDialog } from "./DemolishDialog";
import { EditableSign } from "./EditableSign";
import { NeedsYou } from "./NeedsYou";
import { PLACE_PANEL_ID, PlacePanel } from "./PlacePanel";
import { UsageSigns } from "./UsageSigns";
import { Building, buildingHeight } from "./Building";
import type { FloorModel } from "./Storey";
import type { OverrideActions } from "../../state/overrides";

export interface ScreenOrigin {
  readonly x: number;
  readonly y: number;
}

interface Props {
  readonly snapshot: OfficeSnapshot;
  /** Go inside a floor; `origin` is where the camera dives, relative to the view. */
  readonly onEnter: (projectId: string, origin: ScreenOrigin) => void;
  readonly canPickFolder: boolean;
  readonly onPickFolder: () => Promise<string | null>;
  readonly onAddFloor: (path: string) => Promise<Project>;
  readonly onDemolishFloor: (projectId: string) => Promise<void>;
  readonly loadUsage: (refresh: boolean) => Promise<readonly ToolUsage[]>;
  /** The Add floor or Demolish plate is opening, so no other plate may stay open over it. */
  readonly onOpenPlate: () => void;
  /** The Office/Islands switch, shown in the top bar when given. */
  readonly viewSwitch?: React.ReactNode;
  /** Draw the floors as a building (default) or as islands; everything around the drawing stays the same. */
  readonly mode?: ViewMode;
  /** Open one agent's terminal or conversation, from a place's panel or the Needs-you list. */
  readonly onOpenSession: (session: Session) => void;
  /** Name a place, or move an agent to another one. */
  readonly overrideActions: OverrideActions;
  /** Answer a waiting question from the Needs-you list. */
  readonly onAnswer: (questionId: string, answer: QuestionAnswer) => Promise<void>;
}

// The island map pulls in three.js (a 596 kB chunk, 152 kB gzipped): load it only when Islands is chosen.
const Archipelago = lazy(() => import("../../archipelago/Archipelago").then((m) => ({ default: m.Archipelago })));

/** The stage's side pads in camera units; the left one is where the floors' nameplates stand, 16 of it clear of the building. */
const PAD = { left: 210, right: 187 } as const;
const PLATE_CLEAR = 16;

/** The roof sign's default reads as the place you are looking at; a name of your own shows in both. */
const ISLANDS_DEFAULT_NAME = "The Islands";

/** The sketch's projects screen: the building, a floor search and the active/total tally. */
export function BuildingView({ snapshot, onEnter, canPickFolder, onPickFolder, onAddFloor, onDemolishFloor, loadUsage, onOpenPlate, viewSwitch, mode = "office", onOpenSession, overrideActions, onAnswer }: Props): React.ReactElement {
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState(false);
  const [demolishing, setDemolishing] = useState<string | null>(null);
  const animate = !(useReducedMotion() ?? false);
  const [buildingName, renameBuilding] = useBuildingName();
  const viewRef = useRef<HTMLDivElement>(null);
  /** Office: the floor whose place panel is open. Islands keeps its own selection with its camera. */
  const [placeId, setPlaceId] = useState<string | null>(null);
  const panelHeading = useRef<HTMLHeadingElement>(null);
  /** How far the left pad must grow so the widest nameplate stands inside the window: a long name or a stamp widens its plate, not the pad. */
  const [plateRoom, setPlateRoom] = useState(0);

  const floors: FloorModel[] = snapshot.projects.map((project) => {
    const sessions = sessionsForProject(snapshot, project.id);
    const qa = qaFor(snapshot, project.id);
    return {
      project,
      sessions,
      light: floorLight(sessions),
      counts: sessionCounts(sessions),
      pending: questionsForProject(snapshot, project.id).length,
      dimmed: !matchesQuery(project, query),
      plant: project.id === snapshot.setupProjectId,
      stamp: qa?.enabled === true ? qa.stamp : null,
    };
  });
  const standing = usePresence(floors, (f) => f.project.id, animate);
  const doomed = floors.find((f) => f.project.id === demolishing);
  const matching = floors.filter((f) => !f.dimmed).length;
  const tally = sessionCounts(snapshot.sessions);
  const needsYou = floors.filter((f) => f.light === "red" || f.light === "yellow");
  const width = storeyWidth(Math.max(0, ...floors.map((f) => f.sessions.length)));
  const islands = mode === "islands";
  const shownName = islands && buildingName === DEFAULT_BUILDING_NAME ? ISLANDS_DEFAULT_NAME : buildingName;
  const words = PLACE[mode];
  const openAdding = (): void => {
    onOpenPlate();
    setAdding(true);
  };
  const openDemolishing = (projectId: string): void => {
    onOpenPlate();
    setDemolishing(projectId);
  };

  const originOf = (el: HTMLElement | null): ScreenOrigin => {
    const view = viewRef.current?.getBoundingClientRect();
    if (el === null || view === undefined) return { x: 0, y: 0 };
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2 - view.left, y: r.top + r.height / 2 - view.top };
  };

  const plateSignature = floors.map((f) => `${f.project.name}\u0000${f.stamp ?? ""}\u0000${f.counts.total}`).join("\u0001");
  useLayoutEffect(() => {
    const root = viewRef.current;
    if (root === null || islands) return;
    const measure = (): void => {
      const zoom = parseFloat(getComputedStyle(root.querySelector(".stage-camera") ?? root).zoom) || 1;
      const widest = Math.max(0, ...[...root.querySelectorAll(".storey .nameplate")].map((p) => p.getBoundingClientRect().width / zoom));
      setPlateRoom(Math.max(0, Math.ceil(widest + PLATE_CLEAR) - PAD.left));
    };
    measure();
    // A web font arriving after first paint changes every plate's width.
    const watch = new ResizeObserver(measure);
    for (const plate of root.querySelectorAll(".storey .nameplate")) watch.observe(plate);
    return () => watch.disconnect();
  }, [islands, plateSignature]);
  const placed = mode === "office" ? standing.list.find((e) => e.key === placeId) : undefined;
  useEffect(() => {
    if (placed === undefined) return;
    panelHeading.current?.focus();
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === "Escape") setPlaceId(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [placed?.key]);
  const openAsk = (q: Question): void => {
    const asker = sessionById(snapshot, q.sessionId);
    if (asker !== undefined) onOpenSession(asker);
    else onEnter(q.projectId, originOf(viewRef.current?.querySelector<HTMLElement>(`[data-project="${q.projectId}"]`) ?? null));
  };

  return (
    <div ref={viewRef} className="view building-view">
      <header className="topbar" data-tauri-drag-region>
        <span className="topbar-sign">
          {islands ? (
            <EditableSign name={shownName} onRename={(next) => renameBuilding(next.trim() === ISLANDS_DEFAULT_NAME ? DEFAULT_BUILDING_NAME : next)} noun="map" />
          ) : (
            shownName
          )}
        </span>
        <label className="search">
          <span className="sr-only">Search {words.noun}s</span>
          <input
            type="search"
            value={query}
            placeholder={`Search ${words.noun}s`}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <span className="tally" data-testid="tally" aria-label={`${tally.active} of ${tally.total} agents active`}>
          <b>
            {tally.active}/{tally.total}
          </b>{" "}
          active
        </span>
        {viewSwitch}
        {needsYou.length > 0 ? (
          <span className="needs-you">
            <span>Needs you:</span>
            {needsYou.map((f) => (
              <button
                key={f.project.id}
                type="button"
                className="link"
                onClick={() => {
                  const storey = viewRef.current?.querySelector<HTMLElement>(`[data-project="${f.project.id}"]`) ?? null;
                  onEnter(f.project.id, originOf(storey));
                }}
              >
                {f.project.name}
              </button>
            ))}
          </span>
        ) : null}
        <NeedsYou snapshot={snapshot} onOpen={openAsk} onAnswer={onAnswer} />
      </header>

      {floors.length === 0 ? (
        <p className="scene-note" role="status">
          {words.emptyNote}
        </p>
      ) : query.trim() !== "" && matching === 0 ? (
        <p className="scene-note" role="status">
          No {words.noun}s match “{query.trim()}”.{" "}
          <button type="button" className="link" onClick={() => setQuery("")}>
            Clear search
          </button>
        </p>
      ) : null}

      <div className={islands ? "building-body is-islands" : placed === undefined ? "building-body" : "building-body has-place-panel"}>
        {islands ? (
          <Suspense fallback={null}>
            <Archipelago
              snapshot={snapshot}
              entries={standing.list}
              onGone={standing.done}
              animate={animate}
              onEnter={(projectId) => onEnter(projectId, originOf(viewRef.current))}
              onOpenSession={onOpenSession}
              onRenameIsland={overrideActions.renameFloor}
              onMoveSession={overrideActions.moveSession}
              onAddFloor={openAdding}
              onDemolish={openDemolishing}
            />
          </Suspense>
        ) : (
        <Stage
          camera={BUILDING_CAMERA}
          width={width}
          depth={STOREY.depth}
          height={buildingHeight(standing.list.length)}
          pad={{ left: PAD.left + plateRoom, right: PAD.right + plateRoom, top: 48, bottom: 28 }}
          fit="width"
          maxScale={1.7}
          className="building-stage"
        >
          <Building
            floors={standing.list}
            animate={animate}
            width={width}
            name={buildingName}
            onRename={renameBuilding}
            onEnter={(projectId, el) => onEnter(projectId, originOf(el))}
            onOpenSession={onOpenSession}
            onShow={setPlaceId}
            panelOpenId={placed?.key ?? null}
            onAddFloor={openAdding}
            onDemolish={openDemolishing}
            onGone={standing.done}
          />
        </Stage>
        )}
        {placed === undefined ? null : (
          <PlacePanel
            id={PLACE_PANEL_ID}
            className="office-panel"
            projectId={placed.key}
            name={placed.item.project.name}
            noun={words.noun}
            enterLabel="Enter floor"
            sessions={placed.item.sessions}
            counts={placed.item.counts}
            asks={questionsForProject(snapshot, placed.key)}
            moveTargets={standing.list.filter((e) => e.key !== placed.key && !e.item.plant && e.phase !== "leaving").map((e) => ({ id: e.key, name: e.item.project.name }))}
            leaving={placed.phase === "leaving"}
            headingRef={panelHeading}
            onRename={overrideActions.renameFloor}
            onMove={overrideActions.moveSession}
            onOpenSession={onOpenSession}
            onEnter={(projectId) => onEnter(projectId, originOf(viewRef.current?.querySelector<HTMLElement>(`[data-testid="storey"][data-project="${projectId}"]`) ?? null))}
            onClose={() => setPlaceId(null)}
          />
        )}
        <UsageSigns loadUsage={loadUsage} collapsible view={{ name: mode, startOpen: !islands }} />
      </div>

      {adding ? (
        <AddFloorDialog
          canPickFolder={canPickFolder}
          onPickFolder={onPickFolder}
          onCancel={() => setAdding(false)}
          onAdd={(path) => onAddFloor(path).then(() => setAdding(false))}
          words={words}
        />
      ) : null}

      {doomed === undefined ? null : (
        <DemolishDialog
          name={doomed.project.name}
          agents={doomed.sessions.length}
          onCancel={() => setDemolishing(null)}
          onDemolish={() => onDemolishFloor(doomed.project.id).then(() => setDemolishing(null))}
          words={words}
        />
      )}
    </div>
  );
}
