import { createContext, useContext, useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { DiscoveryApi } from "../../api/DiscoveryApi";
import type { DeskHandoverState, DeskView, ManagerFloor, QueuedNote, Session, Shell } from "../../types";
import { STATE_LABEL, TOOL_LABEL, checkoutLine, modelLine, notesFor, plural } from "../../state/selectors";
import { CloseIcon } from "../icons";
import { shellPaths } from "../../lib/shell";
import { message } from "../../lib/errors";
import { readPref, writePref } from "../../lib/prefs";
import { Conversation } from "./Conversation";
import { useDeskView } from "./useDeskView";
import { OpenInEditor } from "../editor/OpenInEditor";
import { HandoverStrip } from "../handover/HandoverStrip";
import { RouteStrip } from "../routing/RouteStrip";

export type TerminalApi = Pick<DiscoveryApi, "subscribeTerminal" | "writeTerminal" | "resizeTerminal" | "subscribeFileDrops">;

/** The terminal operations, provided by App (the floor view in between does not need them). */
export const TerminalApiContext = createContext<TerminalApi | null>(null);

// xterm.js paints its own canvas, so it needs the colours themselves: these
// mirror --terminal and --terminal-text in src/styles/tokens.css.
const THEME = { background: "#16181c", foreground: "#e8e6e1", cursor: "#e8e6e1", selectionBackground: "#3a3f48" };
const FONT = '"SF Mono", Menlo, Consolas, monospace';

const TERMINAL_HEIGHT_KEY = "office-building:terminal-height";
const CHAT_HEIGHT_KEY = "office-building:chat-height";
const DEFAULT_HEIGHT = 260;
const MIN_HEIGHT = 140;
/** Room the floor keeps above a tall terminal, so its header stays reachable. */
const FLOOR_ROOM = 120;
/** How far one arrow key moves the handle. */
const KEY_STEP = 40;

function storedHeight(key: string, fallback: number | null): number | null {
  const h = Number(readPref(key));
  return Number.isFinite(h) && h >= MIN_HEIGHT ? h : fallback;
}

/**
 * The panel's height, set by dragging its top edge (or arrow keys on it) and remembered between opens
 * under `key`. With nothing stored and no `fallback`, the height is null and the panel fills 60% of the
 * floor until the first move, which starts from its measured height.
 */
function usePanelHeight(
  panel: React.RefObject<HTMLElement | null>,
  key: string,
  fallback: number | null,
): {
  readonly height: number | null;
  readonly handle: React.HTMLAttributes<HTMLDivElement>;
} {
  const [height, setHeight] = useState(() => storedHeight(key, fallback));
  useEffect(() => {
    if (height !== null) writePref(key, String(height));
  }, [key, height]);
  const clamp = (h: number): number => {
    const room = (panel.current?.parentElement?.clientHeight ?? window.innerHeight) - FLOOR_ROOM;
    return Math.round(Math.max(MIN_HEIGHT, Math.min(room, h)));
  };
  const current = (): number => height ?? panel.current?.offsetHeight ?? DEFAULT_HEIGHT;
  const handle: React.HTMLAttributes<HTMLDivElement> = {
    onPointerDown: (e) => {
      e.preventDefault();
      const startY = e.clientY;
      const start = current();
      const move = (ev: PointerEvent): void => setHeight(clamp(start + startY - ev.clientY));
      const up = (): void => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    },
    onKeyDown: (e) => {
      const step = e.key === "ArrowUp" ? KEY_STEP : e.key === "ArrowDown" ? -KEY_STEP : 0;
      if (step === 0) return;
      e.preventDefault();
      setHeight(clamp(current() + step));
    },
  };
  return { height, handle };
}

/** An agent's session, or a plain shell the human opened (its floor's folder alongside). */
type Props = {
  readonly onClose: () => void;
  readonly manager?: ManagerFloor | undefined;
  /** The session's id is in `snapshot.chatDesks`: its panel opens in the chat view. */
  readonly chat?: boolean | undefined;
  /** Your own messages waiting for this session. */
  readonly queued?: readonly QueuedNote[] | undefined;
  /** What this session's handover has to say, if anything. */
  readonly handover?: DeskHandoverState | undefined;
} & ({ readonly session: Session } | { readonly shell: Shell; readonly path: string });

/**
 * A terminal docked under the floor. A plain shell is always live. A session's is:
 *  full        the agent's live in-app terminal (closing the panel leaves it running)
 *  structured  the conversation with an agent the app drives over its protocol
 *  a Claude Code `full` desk also flips between its terminal and a chat drawn from its transcript
 *  read-only   transcript notice
 * raise-window sessions never open here: clicking them raises their own window.
 */
export function TerminalPanel(props: Props): React.ReactElement {
  const { onClose, manager, queued, handover } = props;
  const session = "session" in props ? props.session : null;
  // Opened on a live terminal, the panel stays on it: after `/clear` this id
  // turns read-only while the backend keeps it pointing at the same terminal.
  const [live] = useState(session?.control !== "read-only");
  const panel = useRef<HTMLElement>(null);
  // Claude Code in one of the app's terminals can show either; its panel remembers the choice.
  const flippable = session?.tool === "claude-code" && session.control === "full";
  const [view, setView] = useDeskView(session?.id ?? "", props.chat === true ? "chat" : "terminal");
  const showingChat = session?.control === "structured" || (flippable && view === "chat");
  const terminalHeight = usePanelHeight(panel, TERMINAL_HEIGHT_KEY, DEFAULT_HEIGHT);
  const chatHeight = usePanelHeight(panel, CHAT_HEIGHT_KEY, null);
  const { height, handle } = showingChat ? chatHeight : terminalHeight;
  const id = session?.id ?? ("shell" in props ? props.shell.id : "");
  const title = session?.title ?? ("shell" in props ? `Shell ${props.shell.number}` : "");
  const details =
    session === null
      ? ["path" in props ? props.path : "", "Your shell"]
      : [TOOL_LABEL[session.tool], modelLine(session), STATE_LABEL[session.state]].filter((d) => d !== null);
  const waiting = session === null ? 0 : notesFor(manager, session.id).length;
  return (
    <motion.section
      ref={panel}
      className="terminal"
      style={{ height: height === null ? "60%" : height, maxHeight: `calc(100% - ${FLOOR_ROOM}px)` }}
      data-testid="terminal-panel"
      data-session={id}
      aria-label={`${showingChat ? "Conversation" : "Terminal"}: ${title}`}
      initial={{ y: 40, opacity: 0 }}
      animate={{ y: 0, opacity: 1 }}
      exit={{ y: 40, opacity: 0 }}
      transition={{ duration: 0.2, ease: "easeOut" }}
    >
      <div
        className="terminal-resize"
        role="separator"
        aria-orientation="horizontal"
        aria-label="Resize terminal"
        aria-valuenow={height ?? panel.current?.offsetHeight ?? 0}
        tabIndex={0}
        data-testid="terminal-resize"
        {...handle}
      />
      <header className="terminal-head">
        <span className="terminal-title">
          <b>{title}</b>
          {details.map((d) => (
            <span key={d}>{d}</span>
          ))}
          {session !== null && manager?.managed.includes(session.id) ? (
            <span className="managed-mark managed-mark-dark" data-testid="terminal-managed">
              managed
            </span>
          ) : null}
          {waiting === 0 ? null : <span data-testid="terminal-notes">{plural(waiting, "note", "notes")} from the manager waiting</span>}
        </span>
        <span className="terminal-actions">
          {flippable ? <ViewSwitch view={view} onChange={setView} /> : null}
          {session === null ? null : (
            <OpenInEditor place={{ kind: "session", sessionId: session.id }} what={`“${session.title}”`} opens={checkoutLine(session)} dark shortcut />
          )}
          <button type="button" className="icon-btn icon-btn-dark" aria-label="Close terminal" data-testid="terminal-close" onClick={onClose}>
            <CloseIcon />
          </button>
        </span>
      </header>
      {session !== null && handover !== undefined ? <HandoverStrip sessionId={session.id} state={handover} /> : null}
      {flippable ? <RouteStrip sessionId={session.id} /> : null}
      {session !== null && showingChat ? (
        <Conversation sessionId={session.id} queued={queued} onFlip={flippable ? () => setView("terminal") : undefined} />
      ) : !live ? (
        <p className="terminal-note" data-testid="read-only-note">
          This session was started outside the app and has no window to raise. Showing its transcript read-only.
        </p>
      ) : (
        <LiveTerminal sessionId={id} who={session === null ? "shell" : "agent"} />
      )}
    </motion.section>
  );
}

/** The Chat and Terminal buttons in the head of a Claude Code desk's panel. */
function ViewSwitch({ view, onChange }: { readonly view: DeskView; readonly onChange: (next: DeskView) => void }): React.ReactElement {
  return (
    <span className="view-switch" role="group" aria-label="View" data-testid="view-switch">
      {(["chat", "terminal"] as const).map((v) => (
        <button key={v} type="button" aria-pressed={view === v} onClick={() => onChange(v)}>
          {v === "chat" ? "Chat" : "Terminal"}
        </button>
      ))}
    </span>
  );
}

/** xterm.js wired to the session's terminal: output in, keystrokes and size out. */
function LiveTerminal({ sessionId, who }: { readonly sessionId: string; readonly who: "agent" | "shell" }): React.ReactElement {
  const api = useContext(TerminalApiContext);
  if (api === null) throw new Error("TerminalPanel needs a TerminalApiContext provider");
  const host = useRef<HTMLDivElement>(null);
  const [exit, setExit] = useState<{ readonly code: number | null } | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const el = host.current;
    if (el === null) return;
    const fail = (err: unknown): void => setError(message(err));
    const term = new Terminal({ theme: THEME, fontFamily: FONT, fontSize: 13, cursorBlink: true });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    const typed = term.onData((data) => {
      // A keystroke that gets through means the terminal is reachable again.
      api.writeTerminal(sessionId, data).then(() => setError(null), fail);
    });
    const resized = term.onResize(({ cols, rows }) => {
      api.resizeTerminal(sessionId, cols, rows).catch(fail);
    });
    // Fitting waits for the backlog: resizing first would lose the size it was drawn at.
    let replayed = false;
    // A file dropped on the terminal types its path, as iTerm2 and Terminal.app do.
    const dropped = api.subscribeFileDrops(({ paths, x, y }) => {
      const r = el.getBoundingClientRect();
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) term.paste(shellPaths(paths));
    });
    const observer = new ResizeObserver(() => {
      if (replayed) fit.fit();
    });
    observer.observe(el);
    const stop = api.subscribeTerminal(sessionId, {
      output: (data, drawnAt) => {
        if (drawnAt === undefined) {
          term.write(data);
          return;
        }
        // Replay at the drawn size so the agent's cursor moves land where they did, then fit the panel.
        term.resize(drawnAt.cols, drawnAt.rows);
        term.write(data, () => {
          replayed = true;
          fit.fit();
        });
      },
      exit: (code) => {
        term.options.disableStdin = true;
        setExit({ code });
      },
      error: setError,
    });
    term.focus();
    return () => {
      stop();
      dropped();
      observer.disconnect();
      typed.dispose();
      resized.dispose();
      term.dispose();
    };
  }, [api, sessionId]);

  return (
    <>
      <div className="terminal-screen" ref={host} data-testid="terminal-screen" />
      {exit === null ? null : (
        <p className="terminal-note terminal-status" role="status">
          {exit.code === null ? `The ${who} has exited.` : `The ${who} exited with code ${exit.code}.`} Its last output stays above.
        </p>
      )}
      {error === null ? null : (
        <p className="terminal-note terminal-status" role="alert">
          Terminal problem: {error}.
        </p>
      )}
    </>
  );
}
