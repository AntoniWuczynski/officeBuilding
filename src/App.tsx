import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { AnimatePresence, MotionConfig, motion } from "framer-motion";
import type { DiscoveryApi, HireRequest, QuestionAnswer } from "./api/DiscoveryApi";
import type { Session } from "./types";
import { TOOL_LABEL, sessionById } from "./state/selectors";
import { useNow } from "./state/useNow";
import { useOffice } from "./state/useOffice";
import { BuildingView } from "./components/building/BuildingView";
import type { ScreenOrigin } from "./components/building/BuildingView";
import { FloorView } from "./components/floor/FloorView";
import type { Section } from "./components/floor/sections";
import { TerminalApiContext } from "./components/floor/TerminalPanel";
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
  const now = useNow();
  const [view, setView] = useState<View>({ kind: "building" });
  const [origin, setOrigin] = useState<ScreenOrigin>({ x: 0, y: 0 });
  const [section, setSection] = useState<Section | null>(null);
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

  const openSession = useCallback(
    (session: Session) => {
      if (session.control !== "raise-window") {
        setOpenSessionId(session.id);
        return;
      }
      api
        .focusSession(session.id)
        .then(() => toast("ok", `Brought the ${TOOL_LABEL[session.tool]} window for “${session.title}” to the front.`))
        .catch((err: unknown) => toast("error", `Could not raise the window for “${session.title}”: ${message(err)}.`));
    },
    [api, toast],
  );

  const hire = useCallback(
    (projectId: string, request: HireRequest) =>
      api.spawnSession(projectId, request).then((s) => {
        toast("ok", `Hired ${TOOL_LABEL[s.tool]} (${request.model}, ${request.effort} effort) for “${s.title}”.`);
      }),
    [api, toast],
  );

  const loadHireOptions = useCallback(() => api.hireOptions(), [api]);
  const pickFolder = useCallback(() => api.pickFolder(), [api]);

  const addFloor = useCallback(
    (path: string) => {
      const known = new Set(snapshot?.projects.map((p) => p.id) ?? []);
      return api.addFloor(path).then((project) => {
        toast("ok", known.has(project.id) ? `${project.name} already has a floor.` : `Added the ${project.name} floor.`);
        return project;
      });
    },
    [api, snapshot, toast],
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
              onSection={setSection}
              onOpenSession={openSession}
              onCloseTerminal={() => setOpenSessionId(null)}
              onBack={leave}
              loadHireOptions={loadHireOptions}
              onHire={hire}
              onAnswer={answer}
              onTick={tick}
              onResume={resume}
            />
          </motion.div>
        )}
      </AnimatePresence>
    );
  }

  return (
    <MotionConfig reducedMotion="user">
      <TerminalApiContext value={api}>
        <div className="app">
          {body}
          <Toasts toasts={toasts} onDismiss={dismiss} />
        </div>
      </TerminalApiContext>
    </MotionConfig>
  );
}
