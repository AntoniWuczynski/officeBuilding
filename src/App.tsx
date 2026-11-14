import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { AnimatePresence, MotionConfig, motion } from "framer-motion";
import type { DiscoveryApi, HireRequest, QuestionAnswer } from "./api/DiscoveryApi";
import type { Editor, Place, Session, SettingsPatch } from "./types";
import { TOOL_LABEL, editorLabel, sessionById } from "./state/selectors";
import { useNow } from "./state/useNow";
import { useOffice } from "./state/useOffice";
import { ViewSwitch } from "./components/ViewSwitch";
import { PLACE } from "./state/placeWords";
import { useViewMode } from "./state/useViewMode";
import { BuildingView } from "./components/building/BuildingView";
import type { ScreenOrigin } from "./components/building/BuildingView";
import { FloorView } from "./components/floor/FloorView";
import type { Section } from "./components/floor/sections";
import { TerminalApiContext } from "./components/floor/TerminalPanel";
import { ConversationApiContext } from "./components/floor/Conversation";
import { useEditors } from "./state/useEditors";
import { EditorContext } from "./components/editor/EditorContext";
import type { EditorActions } from "./components/editor/EditorContext";
import { ConnectorsContext } from "./components/connectors/ConnectorsContext";
import type { ConnectorActions } from "./components/connectors/ConnectorsContext";
import { QaContext } from "./components/qa/QaContext";
import type { QaActions } from "./components/qa/QaContext";
import { ManagerContext } from "./components/manager/ManagerContext";
import type { ManagerActions } from "./components/manager/ManagerContext";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { Toasts } from "./components/Toasts";
import type { Toast } from "./components/Toasts";
import { message } from "./lib/errors";

interface Props {
  readonly api: DiscoveryApi;
}

type View = { readonly kind: "building" } | { readonly kind: "floor"; readonly projectId: string };

const CAMERA_EASE = [0.2, 0.8, 0.2, 1] as const;
const TOAST_MS = 4500;

/** Root view-state machine: building ↔ floor, plus the floor's panels and toasts. */
export function App({ api }: Props): React.ReactElement {
  const { state, retry } = useOffice(api);
  const [viewMode, setViewMode] = useViewMode();
  const words = PLACE[viewMode];
  const now = useNow();
  const [view, setView] = useState<View>({ kind: "building" });
  const [origin, setOrigin] = useState<ScreenOrigin>({ x: 0, y: 0 });
  const [section, setSection] = useState<Section | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [openSessionId, setOpenSessionId] = useState<string | null>(null);
  const [toasts, setToasts] = useState<readonly Toast[]>([]);
  const toastSeq = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((ts) => ts.filter((t) => t.id !== id));
  }, []);
  const toast = useCallback(
    (tone: Toast["tone"], text: string) => {
      toastSeq.current += 1;
      const id = toastSeq.current;
      setToasts((ts) => [...ts, { id, tone, text }]);
      setTimeout(() => dismiss(id), TOAST_MS);
    },
    [dismiss],
  );

  const snapshot = state.status === "ready" ? state.snapshot : null;
  const project = view.kind === "floor" && snapshot !== null ? snapshot.projects.find((p) => p.id === view.projectId) : undefined;

  // A hire reported as started can still fail before the agent reaches its desk.
  useEffect(
    () =>
      api.subscribeHireFailures((f) => {
        const code = f.exitCode === null ? "" : ` (exit code ${f.exitCode})`;
        const said = f.lastLines.length === 0 ? "" : `: ${f.lastLines.join(" ")}`;
        toast("error", `“${f.title}” stopped before it reached its desk${code}${said}`);
      }),
    [api, toast],
  );

  // A floor that disappears (project removed upstream) sends you back to the building.
  useEffect(() => {
    if (view.kind === "floor" && snapshot !== null && project === undefined) setView({ kind: "building" });
  }, [view, snapshot, project]);

  const enter = useCallback((projectId: string, at: ScreenOrigin) => {
    // The exiting building must already carry the new camera origin.
    flushSync(() => setOrigin(at));
    setView({ kind: "floor", projectId });
  }, []);

  const leave = useCallback(() => {
    setSection(null);
    setOpenSessionId(null);
    setView({ kind: "building" });
  }, []);

  // The docked terminal and the Settings plate never overlap, so showing one shuts the other.
  const showTerminal = useCallback((id: string) => {
    setSettingsOpen(false);
    setOpenSessionId(id);
  }, []);

  const openSession = useCallback(
    (session: Session) => {
      if (session.control !== "raise-window") {
        showTerminal(session.id);
        return;
      }
      api
        .focusSession(session.id)
        .then(() => toast("ok", `Brought the ${TOOL_LABEL[session.tool]} window for “${session.title}” to the front.`))
        .catch((err: unknown) => toast("error", `Could not raise the window for “${session.title}”: ${message(err)}.`));
    },
    [api, toast, showTerminal],
  );

  const hire = useCallback(
    (projectId: string, request: HireRequest) =>
      api.spawnSession(projectId, request).then((s) => {
        toast("ok", `Hired ${TOOL_LABEL[s.tool]} (${request.model}, ${request.effort} effort) for “${s.title}”.`);
      }),
    [api, toast],
  );

  const newShell = useCallback((projectId: string) => api.openShell(projectId).then((s) => showTerminal(s.id)), [api, showTerminal]);

  // Cmd+N on a floor opens a new shell there, as it opens a window in Terminal.app.
  // Captured first so a focused terminal cannot swallow it.
  const floorId = project?.id;
  useEffect(() => {
    if (floorId === undefined) return;
    const onKey = (e: KeyboardEvent): void => {
      if (!e.metaKey || e.shiftKey || e.altKey || e.ctrlKey || e.key.toLowerCase() !== "n") return;
      e.preventDefault();
      if (e.repeat) return;
      newShell(floorId).catch((err: unknown) => toast("error", `Could not open a shell: ${message(err)}.`));
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, [floorId, newShell, toast]);

  const loadEditors = useCallback(() => api.editors(), [api]);
  const editors = useEditors(loadEditors);
  const reloadEditors = editors.reload;

  const editorList = editors.list;
  const openInEditor = useCallback(
    (place: Place, editor: Editor | null, what: string) => {
      api
        .openInEditor(place, editor)
        .then((used) => toast("ok", `Opened ${what} in ${editorLabel(editorList, used) ?? "your editor"}.`))
        .catch((err: unknown) => {
          const label = editorLabel(editorList, editor ?? editorList?.defaultEditor ?? null) ?? "an editor";
          toast("error", `Could not open ${what} in ${label}: ${message(err)}.`);
          reloadEditors();
        });
    },
    [api, editorList, toast, reloadEditors],
  );
  const editorActions = useMemo<EditorActions>(() => ({ list: editorList, open: openInEditor }), [editorList, openInEditor]);

  // Cmd+Shift+O opens the desk whose panel is open, else the keyboard-focused worker. Captured first so a focused terminal cannot swallow it.
  const sessions = snapshot?.sessions;
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (!e.metaKey || !e.shiftKey || e.altKey || e.ctrlKey || e.key.toLowerCase() !== "o") return;
      const focused = document.activeElement?.closest('[data-testid="worker"]')?.getAttribute("data-session") ?? null;
      const open = sessions?.find((s) => s.id === openSessionId);
      const target = open ?? sessions?.find((s) => s.id === focused);
      if (target === undefined) return;
      e.preventDefault();
      if (e.repeat) return;
      openInEditor({ kind: "session", sessionId: target.id }, editorList?.defaultEditor ?? null, `“${target.title}”`);
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, [sessions, openSessionId, editorList, openInEditor]);

  const openSettings = useCallback(() => {
    // Plates never overlap: the Hire, Add floor and Demolish dialogs must be closed first.
    if (document.querySelector(".dialog:not(.settings-dialog)") !== null) return;
    setSection(null);
    setOpenSessionId(null);
    setSettingsOpen(true);
    reloadEditors();
  }, [reloadEditors]);
  const chooseSection = useCallback((next: Section | null) => {
    if (next !== null) setSettingsOpen(false);
    setSection(next);
  }, []);

  // Cmd+, toggles Settings from the building and from any floor. Captured first so a focused terminal cannot swallow it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (!e.metaKey || e.shiftKey || e.altKey || e.ctrlKey || e.key !== ",") return;
      e.preventDefault();
      if (e.repeat) return;
      if (settingsOpen) setSettingsOpen(false);
      else openSettings();
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, [settingsOpen, openSettings]);

  // Tell the backend which desk is open so it does not alert for it. A failed report only means an extra alert.
  useEffect(() => {
    api.setOpenDesk(openSessionId).catch(() => {});
  }, [api, openSessionId]);

  const loadSettings = useCallback(() => api.getSettings(), [api]);
  const saveSettings = useCallback(
    (patch: SettingsPatch) =>
      api.updateSettings(patch).then((saved) => {
        if (patch.editor !== undefined) reloadEditors();
        return saved;
      }),
    [api, reloadEditors],
  );
  const testAlert = useCallback(() => api.testAlert(), [api]);
  const closeSettings = useCallback(() => setSettingsOpen(false), []);

  const loadHireOptions = useCallback(() => api.hireOptions(), [api]);
  const loadUsage = useCallback((refresh: boolean) => api.toolUsage(refresh), [api]);
  const setup = useMemo(
    () => ({ load: () => api.setupView(), subscribe: api.subscribeSetupChanges.bind(api), onSetTargets: api.setSetupTargets.bind(api), onImport: api.importSetupItem.bind(api) }),
    [api],
  );
  const pickFolder = useCallback(() => api.pickFolder(), [api]);

  const addFloor = useCallback(
    (path: string) => {
      const known = new Set(snapshot?.projects.map((p) => p.id) ?? []);
      return api.addFloor(path).then((project) => {
        toast("ok", words.addedToast(project.name, known.has(project.id)));
        return project;
      });
    },
    [api, snapshot, toast, words],
  );

  const demolishFloor = useCallback(
    (projectId: string) => {
      const name = snapshot?.projects.find((p) => p.id === projectId)?.name;
      return api.demolishFloor(projectId).then(() => toast("ok", words.removeToast(name)));
    },
    [api, snapshot, toast, words],
  );

  const answer = useCallback(
    (questionId: string, a: QuestionAnswer) => {
      const asker = snapshot?.questions.find((q) => q.id === questionId);
      const who = asker === undefined || snapshot === null ? undefined : sessionById(snapshot, asker.sessionId);
      return api.answerQuestion(questionId, a).then(() => {
        toast("ok", who === undefined ? "Answer sent." : `Answer sent. “${who.title}” is back at work.`);
      });
    },
    [api, snapshot, toast],
  );

  const resume = useCallback(
    (sessionId: string) => {
      const title = snapshot?.ended.find((e) => e.id === sessionId)?.title ?? sessionId;
      return api.resumeSession(sessionId).then(() => {
        toast("ok", `Resumed “${title}”. It is heading back to its desk.`);
      });
    },
    [api, snapshot, toast],
  );

  const connectorActions = useMemo<ConnectorActions>(
    () => ({
      keyStatus: () => api.linearKeyStatus(),
      setKey: (key) => api.setLinearKey(key),
      teams: () => api.linearTeams(),
      connect: (projectId, link) => api.connectFloor(projectId, link).then(() => toast("ok", `Connected to Linear team ${link.teamKey}.`)),
      disconnect: (projectId, connector) => api.disconnectFloor(projectId, connector).then(() => toast("ok", "Disconnected from Linear.")),
      recheck: () => {
        api.recheckConnectors().catch((err: unknown) => toast("error", `Could not re-check Linear: ${message(err)}.`));
      },
      openLink: (todoId) => {
        api.openTodoLink(todoId).catch((err: unknown) => toast("error", `Could not open that issue: ${message(err)}.`));
      },
      openSettings,
    }),
    [api, toast, openSettings],
  );

  const qaActions = useMemo<QaActions>(
    () => ({
      setFloor: (projectId, enabled) =>
        api.setFloorQa(projectId, enabled).then(() => toast("ok", enabled ? "QA is on for this floor. The floor is pinned." : "QA is off for this floor.")),
      recheck: () => {
        api.recheckQa().catch((err: unknown) => toast("error", `Could not ask GitHub: ${message(err)}.`));
      },
      openReview: (projectId, review) => {
        api.openReview(projectId, review.number, review.head).catch((err: unknown) => toast("error", `Could not open that review: ${message(err)}.`));
      },
      openSettings,
    }),
    [api, toast, openSettings],
  );

  const managerActions = useMemo<ManagerActions>(() => {
    const change = (done: string, task: Promise<void>): Promise<void> =>
      task.then(
        () => toast("ok", done),
        (err: unknown) => toast("error", `Could not change the manager: ${message(err)}.`),
      );
    return {
      hire: (projectId) =>
        api.hireManager(projectId).then(
          () => toast("ok", "Hired a manager for this floor. The floor is pinned."),
          (err: unknown) => {
            toast("error", `Could not hire a manager: ${message(err)}.`);
            throw err;
          },
        ),
      setPaused: (projectId, paused) =>
        change(paused ? "The manager is paused. Nothing is sent or hired until you resume it." : "The manager is back at work.", api.setManagerPaused(projectId, paused)),
      setHiring: (projectId, allowed) =>
        change(allowed ? "The manager may hire desks on this floor." : "The manager may no longer hire desks on this floor.", api.setManagerHiring(projectId, allowed)),
      withdraw: (projectId, noteId) => {
        api.withdrawManagerNote(projectId, noteId).catch((err: unknown) => toast("error", `Could not withdraw that note: ${message(err)}.`));
      },
      openSettings,
    };
  }, [api, toast, openSettings]);

  const tick = useCallback(
    (todoId: string) => {
      api.tickTodo(todoId).catch((err: unknown) => toast("error", `Could not tick that off: ${message(err)}.`));
    },
    [api, toast],
  );

  let body: React.ReactElement;
  if (state.status === "loading") {
    body = (
      <p className="app-status" role="status">
        Looking for agent sessions…
      </p>
    );
  } else if (state.status === "error") {
    body = (
      <div className="app-status" role="alert">
        <p>Could not reach the session backend: {state.message}.</p>
        <p>Check that the app&apos;s backend is running, then try again.</p>
        <button type="button" className="btn btn-primary" onClick={retry}>
          Try again
        </button>
      </div>
    );
  } else {
    const snap = state.snapshot;
    body = (
      <AnimatePresence mode="wait" initial={false}>
        {project === undefined ? (
          <motion.div
            key="building"
            className="view-frame"
            style={{ transformOrigin: `${origin.x}px ${origin.y}px` }}
            initial={{ opacity: 0, scale: 2.4 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 2.4 }}
            transition={{ duration: 0.5, ease: CAMERA_EASE }}
          >
            <BuildingView
              snapshot={snap}
              onEnter={enter}
              canPickFolder={api.canPickFolder}
              onPickFolder={pickFolder}
              onAddFloor={addFloor}
              onDemolishFloor={demolishFloor}
              loadUsage={loadUsage}
              onOpenPlate={closeSettings}
              mode={viewMode}
              viewSwitch={<ViewSwitch mode={viewMode} onChange={setViewMode} />}
              onOpenSession={(s) => {
                // A session in its own terminal window is raised where it is, as clicking its desk does; others open on their floor.
                if (s.control !== "raise-window") enter(s.projectId, { x: window.innerWidth / 2, y: window.innerHeight / 2 });
                openSession(s);
              }}
            />
          </motion.div>
        ) : (
          <motion.div
            key={`floor-${project.id}`}
            className="view-frame"
            initial={{ opacity: 0, scale: 0.9 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.9 }}
            transition={{ duration: 0.4, ease: CAMERA_EASE }}
          >
            <FloorView
              project={project}
              snapshot={snap}
              now={now}
              section={section}
              openSessionId={openSessionId}
              onSection={chooseSection}
              onOpenSettings={openSettings}
              onOpenPlate={closeSettings}
              onOpenSession={openSession}
              onCloseTerminal={() => setOpenSessionId(null)}
              onBack={leave}
              backLabel={words.back}
              loadHireOptions={loadHireOptions}
              onHire={hire}
              onAnswer={answer}
              onTick={tick}
              onResume={resume}
              onOpenShell={showTerminal}
              onNewShell={() => newShell(project.id)}
              onEndShell={(id) => api.endShell(id)}
              setup={setup}
            />
          </motion.div>
        )}
      </AnimatePresence>
    );
  }

  return (
    <MotionConfig reducedMotion="user">
      <TerminalApiContext value={api}>
      <ConversationApiContext value={api}>
      <EditorContext value={editorActions}>
      <ConnectorsContext value={connectorActions}>
      <QaContext value={qaActions}>
      <ManagerContext value={managerActions}>
        <div className={viewMode === "islands" && project === undefined ? "app is-islands" : "app"}>
          {body}
          {settingsOpen ? <SettingsPanel load={loadSettings} save={saveSettings} editors={editors.list} loadHireOptions={loadHireOptions} onTestAlert={testAlert} onClose={closeSettings} /> : null}
          <Toasts toasts={toasts} onDismiss={dismiss} />
        </div>
      </ManagerContext>
      </QaContext>
      </ConnectorsContext>
      </EditorContext>
      </ConversationApiContext>
      </TerminalApiContext>
    </MotionConfig>
  );
}
