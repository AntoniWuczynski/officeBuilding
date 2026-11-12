import { useState } from "react";
import { AnimatePresence } from "framer-motion";
import type { OfficeSnapshot, Project, Session, ToolOptions } from "../../types";
import type { HireRequest, QuestionAnswer } from "../../api/DiscoveryApi";
import {
  endedForProject,
  qaFor,
  formatUsd,
  openCount,
  questionsForProject,
  searchSessions,
  sessionCounts,
  sessionsForProject,
  splitReviewer,
  todosForProject,
  totalSpend,
} from "../../state/selectors";
import { FLOOR_CAMERA } from "../../lib/iso";
import { ROOM, roomDepth } from "../../lib/layout";
import { Stage } from "../../scene/Stage";
import { LiftIcon } from "../icons";
import { HireDialog } from "./HireDialog";
import { Rail } from "./Rail";
import { Room } from "./Room";
import type { RoomObjectsModel } from "./RoomObjects";
import { Sidebar } from "./Sidebar";
import { TerminalPanel } from "./TerminalPanel";
import { sectionsFor } from "./sections";
import type { Section } from "./sections";
import type { SetupProps } from "./SetupPanel";

interface Props {
  readonly project: Project;
  readonly snapshot: OfficeSnapshot;
  readonly now: Date;
  readonly section: Section | null;
  readonly openSessionId: string | null;
  readonly onSection: (section: Section | null) => void;
  readonly onOpenSettings: () => void;
  /** The Hire plate is opening, so no other plate may stay open over it. */
  readonly onOpenPlate: () => void;
  readonly onOpenSession: (session: Session) => void;
  readonly onCloseTerminal: () => void;
  readonly onBack: () => void;
  readonly loadHireOptions: () => Promise<readonly ToolOptions[]>;
  readonly onHire: (projectId: string, request: HireRequest) => Promise<void>;
  readonly onAnswer: (questionId: string, answer: QuestionAnswer) => Promise<void>;
  readonly onTick: (todoId: string) => void;
  readonly onResume: (sessionId: string) => Promise<void>;
  /** Show one of the floor's shells in the terminal panel. */
  readonly onOpenShell: (shellId: string) => void;
  /** Open a new shell on this floor and show it. */
  readonly onNewShell: () => Promise<void>;
  readonly onEndShell: (shellId: string) => Promise<void>;
  readonly setup: SetupProps;
}

/** The sketch's project screen: the open-plan floor, its search and tally, the rail and panels. */
export function FloorView(props: Props): React.ReactElement {
  const { project, snapshot, now, openSessionId, onSection, onOpenSession, onCloseTerminal, onBack, onOpenSettings, onOpenPlate, loadHireOptions, onHire, onAnswer, onTick, onResume, onOpenShell, onNewShell, onEndShell, setup } = props;
  const setupFloor = snapshot.setupProjectId === project.id;
  // Only the setup floor has a Setup panel, even if one was left open on it.
  const section = props.section === "setup" && !setupFloor ? null : props.section;
  const [query, setQuery] = useState("");
  const [hireOpen, setHireOpen] = useState(false);

  const sessions = sessionsForProject(snapshot, project.id);
  const counts = sessionCounts(sessions);
  const matching = searchSessions(sessions, query).length;
  const questions = questionsForProject(snapshot, project.id);
  const humanTodos = todosForProject(snapshot, project.id, "human");
  const aiTodos = todosForProject(snapshot, project.id, "ai");
  const spend = totalSpend(sessions);
  const ended = endedForProject(snapshot, project.id);
  const qa = qaFor(snapshot, project.id);
  const qaDesk = qa !== undefined && qa.enabled;
  const { seated, reviewer } = splitReviewer(sessions, qa);
  const depth = roomDepth(seated.length + 1, qaDesk);
  const openSession = sessions.find((s) => s.id === openSessionId);
  const shells = snapshot.shells.filter((s) => s.projectId === project.id);
  const openShell = shells.find((s) => s.id === openSessionId);

  const objects: RoomObjectsModel = {
    projectName: project.name,
    path: project.path,
    counts,
    decisionsWaiting: questions.length,
    humanTodos,
    aiTodos,
    spend,
    signedOut: ended.length,
    shells: shells.length,
    plant: setupFloor,
  };

  const badge = (n: number): string | null => (n > 0 ? String(n) : null);

  return (
    <div className="view floor-view">
      <header className="topbar" data-tauri-drag-region>
        <button type="button" className="btn btn-back" data-testid="back-button" onClick={onBack}>
          <LiftIcon />
          Building
        </button>
        <h1 className="floor-name">{project.name}</h1>
        <label className="search">
          <span className="sr-only">Search desks</span>
          <input type="search" value={query} placeholder="Search desks" onChange={(e) => setQuery(e.target.value)} />
        </label>
        <span className="tally" data-testid="tally" aria-label={`${counts.active} of ${counts.total} agents active on this floor`}>
          <b>
            {counts.active}/{counts.total}
          </b>{" "}
          active
        </span>
      </header>

      <div className="floor-body">
        <main className="floor-main">
          {sessions.length === 0 ? (
            <p className="scene-note" role="status">
              Nobody works on this floor yet. Hire the first agent, or start one in <span className="mono">{project.path}</span> and it moves in.
            </p>
          ) : query.trim() !== "" && matching === 0 ? (
            <p className="scene-note" role="status">
              No desks match “{query.trim()}”.{" "}
              <button type="button" className="link" onClick={() => setQuery("")}>
                Clear search
              </button>
            </p>
          ) : null}

          <Stage
            camera={FLOOR_CAMERA}
            width={ROOM.width}
            depth={depth}
            height={ROOM.wallHeight + ROOM.slab}
            pad={{ left: 24, right: 24, top: 36, bottom: 40 }}
            fit="contain"
            zoomable
            className="room-stage"
          >
            <Room
              sessions={seated}
              qa={qaDesk ? qa : null}
              reviewer={reviewer}
              depth={depth}
              query={query}
              now={now}
              messages={snapshot.messages}
              objects={objects}
              hiring={hireOpen}
              onOpenSession={onOpenSession}
              onOpenSection={(s) => onSection(s)}
              onOpenSettings={onOpenSettings}
              onHire={() => {
                onOpenPlate();
                setHireOpen(true);
              }}
            />
          </Stage>

          {hireOpen ? (
            <HireDialog
              projectName={project.name}
              loadOptions={loadHireOptions}
              onCancel={() => setHireOpen(false)}
              onHire={(request) => onHire(project.id, request).then(() => setHireOpen(false))}
            />
          ) : null}

          <AnimatePresence>
            {openSession !== undefined ? (
              <TerminalPanel key={openSession.id} session={openSession} onClose={onCloseTerminal} />
            ) : openShell !== undefined ? (
              <TerminalPanel key={openShell.id} shell={openShell} path={project.path} onClose={onCloseTerminal} />
            ) : null}
          </AnimatePresence>
        </main>

        <AnimatePresence>
          {section === null ? null : (
            <Sidebar
              key="sidebar"
              section={section}
              project={project}
              snapshot={snapshot}
              now={now}
              onClose={() => onSection(null)}
              onAnswer={onAnswer}
              onTick={onTick}
              onOpenSession={onOpenSession}
              onResume={onResume}
              onOpenShell={onOpenShell}
              onNewShell={onNewShell}
              onEndShell={onEndShell}
              setup={setup}
            />
          )}
        </AnimatePresence>

        <Rail
          sections={sectionsFor(setupFloor)}
          active={section}
          onSelect={onSection}
          badges={{
            info: null,
            decisions: badge(questions.length),
            "human-todo": badge(openCount(humanTodos)),
            todo: badge(openCount(aiTodos)),
            spend: formatUsd(spend.usd),
            "signed-out": badge(ended.length),
            shells: badge(shells.length),
          }}
        />
      </div>
    </div>
  );
}
