import { createContext, useContext, useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { DiscoveryApi } from "../../api/DiscoveryApi";
import type { Session } from "../../types";
import { STATE_LABEL, TOOL_LABEL, modelLine } from "../../state/selectors";
import { CloseIcon } from "../icons";
import { shellPaths } from "../../lib/shell";
import { message } from "../../lib/errors";
import { readPref, writePref } from "../../lib/prefs";

export type TerminalApi = Pick<DiscoveryApi, "subscribeTerminal" | "writeTerminal" | "resizeTerminal" | "subscribeFileDrops">;

/** The terminal operations, provided by App (the floor view in between does not need them). */
export const TerminalApiContext = createContext<TerminalApi | null>(null);

// xterm.js paints its own canvas, so it needs the colours themselves: these
// mirror --terminal and --terminal-text in src/styles/tokens.css.
const THEME = { background: "#16181c", foreground: "#e8e6e1", cursor: "#e8e6e1", selectionBackground: "#3a3f48" };
const FONT = '"SF Mono", Menlo, Consolas, monospace';

const HEIGHT_KEY = "office-building:terminal-height";
const DEFAULT_HEIGHT = 260;
const MIN_HEIGHT = 140;
/** Room the floor keeps above a tall terminal, so its header stays reachable. */
const FLOOR_ROOM = 120;
/** How far one arrow key moves the handle. */
const KEY_STEP = 40;

function storedHeight(): number {
  const h = Number(readPref(HEIGHT_KEY));
  return Number.isFinite(h) && h >= MIN_HEIGHT ? h : DEFAULT_HEIGHT;
}

/** The panel's height, set by dragging its top edge (or arrow keys on it) and remembered between opens. */
function usePanelHeight(panel: React.RefObject<HTMLElement | null>): {
  readonly height: number;
  readonly handle: React.HTMLAttributes<HTMLDivElement>;
} {
  const [height, setHeight] = useState(storedHeight);
  useEffect(() => writePref(HEIGHT_KEY, String(height)), [height]);
  const clamp = (h: number): number => {
    const room = (panel.current?.parentElement?.clientHeight ?? window.innerHeight) - FLOOR_ROOM;
    return Math.round(Math.max(MIN_HEIGHT, Math.min(room, h)));
  };
  const handle: React.HTMLAttributes<HTMLDivElement> = {
    onPointerDown: (e) => {
      e.preventDefault();
      const startY = e.clientY;
      const move = (ev: PointerEvent): void => setHeight(clamp(height + startY - ev.clientY));
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
      setHeight(clamp(height + step));
    },
  };
  return { height, handle };
}

interface Props {
  readonly session: Session;
  readonly onClose: () => void;
}

/**
 * The session's terminal, docked under the floor:
 *  full       the agent's live in-app terminal (closing the panel leaves it running)
 *  read-only  transcript notice
 * raise-window sessions never open here: clicking them raises their own window.
 */
export function TerminalPanel({ session, onClose }: Props): React.ReactElement {
  // Opened on a live terminal, the panel stays on it: after `/clear` this id
  // turns read-only while the backend keeps it pointing at the same terminal.
  const [live] = useState(session.control !== "read-only");
  const panel = useRef<HTMLElement>(null);
  const { height, handle } = usePanelHeight(panel);
  return (
    <motion.section
      ref={panel}
      className="terminal"
      style={{ height, maxHeight: `calc(100% - ${FLOOR_ROOM}px)` }}
      data-testid="terminal-panel"
      data-session={session.id}
      aria-label={`Terminal: ${session.title}`}
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
        aria-valuenow={height}
        tabIndex={0}
        data-testid="terminal-resize"
        {...handle}
      />
      <header className="terminal-head">
        <span className="terminal-title">
          <b>{session.title}</b>
          <span>{TOOL_LABEL[session.tool]}</span>
          {modelLine(session) === null ? null : <span>{modelLine(session)}</span>}
          <span>{STATE_LABEL[session.state]}</span>
        </span>
        <button type="button" className="icon-btn icon-btn-dark" aria-label="Close terminal" data-testid="terminal-close" onClick={onClose}>
          <CloseIcon />
        </button>
      </header>
      {!live ? (
        <p className="terminal-note" data-testid="read-only-note">
          This session was started outside the app and has no window to raise. Showing its transcript read-only.
        </p>
      ) : (
        <LiveTerminal sessionId={session.id} />
      )}
    </motion.section>
  );
}

/** xterm.js wired to the session's terminal: output in, keystrokes and size out. */
function LiveTerminal({ sessionId }: { readonly sessionId: string }): React.ReactElement {
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
          {exit.code === null ? "The agent has exited." : `The agent exited with code ${exit.code}.`} Its last output stays above.
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
