import { useRef, useState } from "react";
import type { OfficeSnapshot, Project } from "../../types";
import {
  floorLight,
  matchesQuery,
  questionsForProject,
  sessionCounts,
  sessionsForProject,
} from "../../state/selectors";
import { BUILDING_CAMERA } from "../../lib/iso";
import { STOREY, storeyWidth } from "../../lib/layout";
import { Stage } from "../../scene/Stage";
import { AddFloorDialog } from "./AddFloorDialog";
import { Building, buildingHeight } from "./Building";
import type { FloorModel } from "./Storey";

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
}

/** The sketch's projects screen: the building, a floor search and the active/total tally. */
export function BuildingView({ snapshot, onEnter, canPickFolder, onPickFolder, onAddFloor }: Props): React.ReactElement {
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState(false);
  const viewRef = useRef<HTMLDivElement>(null);

  const floors: FloorModel[] = snapshot.projects.map((project) => {
    const sessions = sessionsForProject(snapshot, project.id);
    return {
      project,
      sessions,
      light: floorLight(sessions),
      counts: sessionCounts(sessions),
      pending: questionsForProject(snapshot, project.id).length,
      dimmed: !matchesQuery(project, query),
    };
  });
  const matching = floors.filter((f) => !f.dimmed).length;
  const tally = sessionCounts(snapshot.sessions);
  const needsYou = floors.filter((f) => f.light === "red" || f.light === "yellow");
  const width = storeyWidth(Math.max(0, ...floors.map((f) => f.sessions.length)));

  const originOf = (el: HTMLElement | null): ScreenOrigin => {
    const view = viewRef.current?.getBoundingClientRect();
    if (el === null || view === undefined) return { x: 0, y: 0 };
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2 - view.left, y: r.top + r.height / 2 - view.top };
  };

  return (
    <div ref={viewRef} className="view building-view">
      <header className="topbar" data-tauri-drag-region>
        <span className="topbar-sign">Office Building</span>
        <label className="search">
          <span className="sr-only">Search floors</span>
          <input
            type="search"
            value={query}
            placeholder="Search floors"
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <span className="tally" data-testid="tally" aria-label={`${tally.active} of ${tally.total} agents active`}>
          <b>
            {tally.active}/{tally.total}
          </b>{" "}
          active
        </span>
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
      </header>

      {floors.length === 0 ? (
        <p className="scene-note" role="status">
          No projects yet. When an agent starts work in a repo, its floor appears here, or add a floor from the roof.
        </p>
      ) : query.trim() !== "" && matching === 0 ? (
        <p className="scene-note" role="status">
          No floors match “{query.trim()}”.{" "}
          <button type="button" className="link" onClick={() => setQuery("")}>
            Clear search
          </button>
        </p>
      ) : null}

      <Stage
        camera={BUILDING_CAMERA}
        width={width}
        depth={STOREY.depth}
        height={buildingHeight(floors.length)}
        pad={{ left: 210, right: 28, top: 48, bottom: 28 }}
        fit="width"
        maxScale={1.7}
        className="building-stage"
      >
        <Building
          floors={floors}
          width={width}
          onEnter={(projectId, el) => onEnter(projectId, originOf(el))}
          onAddFloor={() => setAdding(true)}
        />
      </Stage>

      {adding ? (
        <AddFloorDialog
          canPickFolder={canPickFolder}
          onPickFolder={onPickFolder}
          onCancel={() => setAdding(false)}
          onAdd={(path) => onAddFloor(path).then(() => setAdding(false))}
        />
      ) : null}
    </div>
  );
}
