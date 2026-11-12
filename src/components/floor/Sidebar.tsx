import { motion } from "framer-motion";
import type { Connection, OfficeSnapshot, Project, QaFloor, Session } from "../../types";
import type { QuestionAnswer } from "../../api/DiscoveryApi";
import {
  STATE_LABEL,
  TOOL_LABEL,
  connectionFor,
  qaFor,
  decisionsForProject,
  endedForProject,
  formatTokens,
  formatUsd,
  questionsForProject,
  sessionCounts,
  sessionsForProject,
  todosForProject,
  totalSpend,
} from "../../state/selectors";
import { relativeTime } from "../../lib/time";
import { CloseIcon } from "../icons";
import { LinearRow } from "../connectors/LinearRow";
import { QaRow } from "../qa/QaRow";
import { DecisionsPanel } from "./DecisionsPanel";
import { TodoList } from "./TodoList";
import { SetupPanel } from "./SetupPanel";
import type { SetupProps } from "./SetupPanel";
import { ShellsPanel } from "./ShellsPanel";
import { SignOutSheet } from "./SignOutSheet";
import { OpenInEditor } from "../editor/OpenInEditor";
import { sectionLabel } from "./sections";
import type { Section } from "./sections";

interface Props {
  readonly section: Section;
  readonly project: Project;
  readonly snapshot: OfficeSnapshot;
  readonly now: Date;
  readonly onClose: () => void;
  readonly onAnswer: (questionId: string, answer: QuestionAnswer) => Promise<void>;
  readonly onTick: (todoId: string) => void;
  readonly onOpenSession: (session: Session) => void;
  readonly onResume: (sessionId: string) => Promise<void>;
  readonly onOpenShell: (shellId: string) => void;
  readonly onNewShell: () => Promise<void>;
  readonly onEndShell: (shellId: string) => Promise<void>;
  readonly setup: SetupProps;
}

/** The enlarged sidebar: one of the manager's sections for this floor. */
export function Sidebar({ onClose, ...body }: Props): React.ReactElement {
  const { section } = body;
  return (
    <motion.aside
      className="sidebar"
      data-testid="sidebar"
      aria-label={sectionLabel(section)}
      initial={{ x: 24, opacity: 0 }}
      animate={{ x: 0, opacity: 1 }}
      exit={{ x: 24, opacity: 0 }}
      transition={{ duration: 0.18, ease: "easeOut" }}
    >
      <header className="panel-head">
        <h2>{sectionLabel(section)}</h2>
        <button type="button" className="icon-btn" aria-label="Close panel" data-testid="sidebar-close" onClick={onClose}>
          <CloseIcon />
        </button>
      </header>
      <SectionBody {...body} />
    </motion.aside>
  );
}

function SectionBody({ section, project, snapshot, now, onAnswer, onTick, onOpenSession, onResume, onOpenShell, onNewShell, onEndShell, setup }: Omit<Props, "onClose">): React.ReactElement {
  const sessions = sessionsForProject(snapshot, project.id);
  switch (section) {
    case "setup":
      return <SetupPanel {...setup} />;
    case "decisions":
      return (
        <DecisionsPanel
          key={project.id}
          snapshot={snapshot}
          questions={questionsForProject(snapshot, project.id)}
          decisions={decisionsForProject(snapshot, project.id)}
          now={now}
          onAnswer={onAnswer}
          onOpenSession={onOpenSession}
        />
      );
    case "human-todo":
      return (
        <TodoList
          items={todosForProject(snapshot, project.id, "human")}
          empty="Nothing for you to do on this floor."
          hint="Tick an item off when you have done it."
          connection={connectionFor(snapshot, project.id)}
          now={now}
          onTick={onTick}
        />
      );
    case "todo":
      return (
        <TodoList
          items={todosForProject(snapshot, project.id, "ai")}
          empty="The agents have nothing queued."
          hint="Agents tick these off as they finish. You can tick one off yourself."
          connection={connectionFor(snapshot, project.id)}
          now={now}
          onTick={onTick}
        />
      );
    case "spend":
      return <SpendBody sessions={sessions} allSessions={snapshot.sessions} onOpenSession={onOpenSession} />;
    case "info":
      return <InfoBody project={project} sessions={sessions} now={now} connection={connectionFor(snapshot, project.id)} qa={qaFor(snapshot, project.id)} onOpenSession={onOpenSession} />;
    case "signed-out":
      return <SignOutSheet ended={endedForProject(snapshot, project.id)} now={now} onResume={onResume} />;
    case "shells":
      return <ShellsPanel shells={snapshot.shells.filter((s) => s.projectId === project.id)} now={now} onOpenShell={onOpenShell} onNewShell={onNewShell} onEndShell={onEndShell} />;
  }
}

function SpendBody({ sessions, allSessions, onOpenSession }: { readonly sessions: readonly Session[]; readonly allSessions: readonly Session[]; readonly onOpenSession: (s: Session) => void }): React.ReactElement {
  const floor = totalSpend(sessions);
  const building = totalSpend(allSessions);
  const ranked = [...sessions].sort((a, b) => b.spend.usd - a.spend.usd);
  return (
    <div className="panel-body">
      <p className="spend-total" data-testid="spend-total">
        <b>{formatUsd(floor.usd)}</b>
        <span>{formatTokens(floor.tokens)} on this floor</span>
      </p>
      <p className="panel-hint">
        {formatUsd(building.usd)} across the building, at API list prices.
        {floor.unpricedTokens > 0 ? ` ${formatTokens(floor.unpricedTokens)} on this floor came from models with no public price and are not in the total.` : ""}
      </p>
      {ranked.length === 0 ? (
        <p className="panel-empty">No agents, no spend.</p>
      ) : (
        <ul className="spend-list">
          {ranked.map((s) => (
            <li key={s.id}>
              <button type="button" className="spend-row" onClick={() => onOpenSession(s)}>
                <span className="spend-name">{s.title}</span>
                <span className="spend-tool">{TOOL_LABEL[s.tool]}</span>
                <span className="spend-usd">{formatUsd(s.spend.usd)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface InfoProps {
  readonly project: Project;
  readonly sessions: readonly Session[];
  readonly now: Date;
  readonly connection: Connection | undefined;
  readonly qa: QaFloor | undefined;
  readonly onOpenSession: (session: Session) => void;
}

function InfoBody({ project, sessions, now, connection, qa, onOpenSession }: InfoProps): React.ReactElement {
  const counts = sessionCounts(sessions);
  const latest = [...sessions].sort((a, b) => b.lastActivityAt.localeCompare(a.lastActivityAt))[0];
  const tools = [...new Set(sessions.map((s) => TOOL_LABEL[s.tool]))];
  const byState = [...new Set(sessions.map((s) => s.state))].map((state) => ({
    label: STATE_LABEL[state],
    n: sessions.filter((s) => s.state === state).length,
  }));
  return (
    <div className="panel-body">
      <dl className="info-list">
        <dt>Project</dt>
        <dd>{project.name}</dd>
        <dt>Folder</dt>
        <dd className="mono">{project.path}</dd>
        <dt>Code</dt>
        <dd>
          <OpenInEditor place={{ kind: "floor", projectId: project.id }} what={`the ${project.name} floor`} opens="this floor's folder" />
        </dd>
        <LinearRow project={project} connection={connection} />
        <QaRow project={project} qa={qa} sessions={sessions} now={now} onOpenSession={onOpenSession} />
        <dt>Agents</dt>
        <dd>
          {counts.active} of {counts.total} active
        </dd>
        {byState.length > 0 ? (
          <>
            <dt>Right now</dt>
            <dd>{byState.map((s) => `${s.n} ${s.label.toLowerCase()}`).join(", ")}</dd>
          </>
        ) : null}
        {tools.length > 0 ? (
          <>
            <dt>Tools</dt>
            <dd>{tools.join(", ")}</dd>
          </>
        ) : null}
        <dt>Last activity</dt>
        <dd>{latest === undefined ? "None yet" : `${latest.title}, ${relativeTime(latest.lastActivityAt, now)}`}</dd>
      </dl>
    </div>
  );
}
