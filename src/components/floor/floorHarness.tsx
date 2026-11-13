import { MockApi } from "../../api/mockApi";
import type { ManagerFloor, OfficeSnapshot, Session } from "../../types";
import { FloorView } from "./FloorView";
import type { Section } from "./sections";

const api = new MockApi();

export const NOW = new Date("2026-09-28T15:45:00Z");

interface Options {
  readonly onSection?: (s: Section | null) => void;
  readonly section?: Section | null;
}

/** A floor of the snapshot, as the tests of the floor's objects draw it. */
export function floor(snapshot: OfficeSnapshot, projectId: string, { onSection = () => {}, section = null }: Options = {}): React.ReactElement {
  const project = snapshot.projects.find((p) => p.id === projectId);
  if (project === undefined) throw new Error(`no project ${projectId}`);
  return (
    <FloorView
      project={project}
      snapshot={snapshot}
      now={NOW}
      section={section}
      openSessionId={null}
      onSection={onSection}
      onOpenSettings={() => {}}
      onOpenPlate={() => {}}
      onOpenSession={() => {}}
      onCloseTerminal={() => {}}
      onBack={() => {}}
      loadHireOptions={() => Promise.resolve([])}
      onHire={() => Promise.resolve()}
      onAnswer={() => Promise.resolve()}
      onTick={() => {}}
      onResume={() => Promise.resolve()}
      onOpenShell={() => {}}
      onNewShell={() => Promise.resolve()}
      onEndShell={() => Promise.resolve()}
      setup={{
        load: () => api.setupView(),
        subscribe: (l) => api.subscribeSetupChanges(l),
        onSetTargets: (k, n, t) => api.setSetupTargets(k, n, t),
        onImport: (f, k, n) => api.importSetupItem(f, k, n),
      }}
    />
  );
}

/** The snapshot with one floor's manager changed. */
export function withManager(snapshot: OfficeSnapshot, projectId: string, change: (m: ManagerFloor) => ManagerFloor): OfficeSnapshot {
  return { ...snapshot, managers: snapshot.managers.map((m) => (m.projectId === projectId ? change(m) : m)) };
}

/** A manager that has been bound to its session `sessionId`. */
export function boundTo(sessionId: string): (m: ManagerFloor) => ManagerFloor {
  return (m) => ({ ...m, status: { kind: "hired", sessionId, startedAt: "2026-09-28T15:00:00Z", lastCallAt: null } });
}

/** A Claude Code session that is the manager of `projectId`. */
export function managerSession(projectId: string, id = "m-1"): Session {
  return {
    id,
    tool: "claude-code",
    projectId,
    title: "floor manager",
    state: "running",
    control: "full",
    model: null,
    effort: null,
    lastActivityAt: "2026-09-28T15:40:00Z",
    pendingQuestionIds: [],
    spend: { usd: 0, tokens: 0, unpricedTokens: 0 },
    helpers: [],
    pets: [],
    worktree: false,
    branch: null,
  };
}
