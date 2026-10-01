import { useState } from "react";
import { motion } from "framer-motion";
import type { OfficeSnapshot, Project, Session, TodoItem } from "../../types";
import type { QuestionAnswer } from "../../api/DiscoveryApi";
import {
  STATE_LABEL,
  TOOL_LABEL,
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
import { DecisionsPanel } from "./DecisionsPanel";
import { SignOutSheet } from "./SignOutSheet";
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
}

/** The enlarged sidebar: one of the manager's sections for this floor. */
export function Sidebar({ section, project, snapshot, now, onClose, onAnswer, onTick, onOpenSession, onResume }: Props): React.ReactElement {
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
      <SectionBody
        section={section}
        project={project}
        snapshot={snapshot}
        now={now}
        onAnswer={onAnswer}
        onTick={onTick}
        onOpenSession={onOpenSession}
        onResume={onResume}
      />
    </motion.aside>
  );
}

function SectionBody({ section, project, snapshot, now, onAnswer, onTick, onOpenSession, onResume }: Omit<Props, "onClose">): React.ReactElement {
  const sessions = sessionsForProject(snapshot, project.id);
  switch (section) {
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
          onTick={onTick}
        />
      );
    case "todo":
      return (
        <TodoList
          items={todosForProject(snapshot, project.id, "ai")}
          empty="The agents have nothing queued."
          hint="Agents tick these off as they finish. You can tick one off yourself."
          onTick={onTick}
        />
      );
    case "spend":
      return <SpendBody sessions={sessions} allSessions={snapshot.sessions} onOpenSession={onOpenSession} />;
    case "info":
      return <InfoBody project={project} sessions={sessions} now={now} />;
    case "signed-out":
      return <SignOutSheet ended={endedForProject(snapshot, project.id)} now={now} onResume={onResume} />;
  }
}

/** Open items shown before "Show all" (real TODO.md files can pass 2,000 items). */
const OPEN_LIMIT = 200;

function TodoList({ items, empty, hint, onTick }: { readonly items: readonly TodoItem[]; readonly empty: string; readonly hint: string; readonly onTick: (id: string) => void }): React.ReactElement {
  const [showDone, setShowDone] = useState(false);
  const [showAllOpen, setShowAllOpen] = useState(false);
  const open = items.filter((t) => !t.done);
  const done = items.filter((t) => t.done);
  const visibleOpen = showAllOpen ? open : open.slice(0, OPEN_LIMIT);
  const shown = showDone ? [...visibleOpen, ...done] : visibleOpen;
  return (
    <div className="panel-body">
      {items.length === 0 ? (
        <p className="panel-empty">{empty}</p>
      ) : (
        <>
          <p className="panel-hint">
            {items[0]?.sourceFile == null ? hint : `From ${items[0].sourceFile}. Ticking an item writes [x] into the file.`}
          </p>
          {open.length === 0 ? <p className="panel-empty">Nothing open.</p> : null}
          <ul className="todo-list">
            {shown.map((t) => (
              <li key={t.id} className={t.done ? "is-done" : undefined}>
                <label>
                  <input type="checkbox" data-testid="todo-check" data-todo={t.id} checked={t.done} disabled={t.done} onChange={() => onTick(t.id)} />
                  <span>{t.text}</span>
                </label>
              </li>
            ))}
          </ul>
          {open.length > visibleOpen.length ? (
            <button type="button" className="link" onClick={() => setShowAllOpen(true)}>
              Show all {open.length} open items
            </button>
          ) : null}
          {done.length > 0 ? (
            <button type="button" className="link" data-testid="toggle-done" onClick={() => setShowDone((v) => !v)}>
              {showDone ? `Hide ${done.length} done` : `Show ${done.length} done`}
            </button>
          ) : null}
        </>
      )}
    </div>
  );
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

function InfoBody({ project, sessions, now }: { readonly project: Project; readonly sessions: readonly Session[]; readonly now: Date }): React.ReactElement {
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
