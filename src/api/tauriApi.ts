import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { OfficeSnapshot, Project, Session, ToolOptions } from "../types";
import type { z } from "zod";
import type { DiscoveryApi, FileDrop, HireFailure, HireRequest, QuestionAnswer, TerminalHandlers } from "./DiscoveryApi";
import {
  folderSchema,
  hireFailureSchema,
  hireOptionsSchema,
  parseWire,
  projectSchema,
  sessionSchema,
  snapshotSchema,
  terminalBacklogSchema,
  terminalExitSchema,
  terminalOutputSchema,
} from "./wire";

/** Pushed by src-tauri/src/watch.rs whenever discovery sees a change. */
const SNAPSHOT_EVENT = "office://snapshot";
/** Pushed by src-tauri/src/pty.rs for every in-app terminal. */
const TERMINAL_OUTPUT_EVENT = "office://terminal-output";
const TERMINAL_EXIT_EVENT = "office://terminal-exit";
/** Pushed by src-tauri/src/office.rs when a hired agent exits before reaching a desk. */
const HIRE_FAILED_EVENT = "office://hire-failed";

type TerminalLive =
  | ({ readonly kind: "output" } & z.infer<typeof terminalOutputSchema>)
  | ({ readonly kind: "exit" } & z.infer<typeof terminalExitSchema>);

/** Tauri commands reject with the Rust error string. */
function asError(err: unknown): Error {
  return err instanceof Error ? err : new Error(String(err));
}

async function call(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  try {
    return await invoke<unknown>(cmd, args);
  } catch (err: unknown) {
    throw asError(err);
  }
}

/**
 * Deliver what `start`'s listener hears until the returned stop is called,
 * even when that happens before the listener has attached.
 */
function follow<T>(start: (deliver: (value: T) => void) => Promise<() => void>, listener: (value: T) => void): () => void {
  let stopped = false;
  let unlisten: (() => void) | null = null;
  void start((value) => {
    if (!stopped) listener(value);
  }).then((off) => {
    if (stopped) off();
    else unlisten = off;
  });
  return () => {
    stopped = true;
    unlisten?.();
  };
}

/** {@link DiscoveryApi} backed by the Rust side (src-tauri/src/commands.rs). */
export class TauriApi implements DiscoveryApi {
  async getSnapshot(): Promise<OfficeSnapshot> {
    return parseWire(snapshotSchema, "snapshot", await call("get_snapshot"));
  }

  subscribe(listener: (snapshot: OfficeSnapshot) => void): () => void {
    let stopped = false;
    let unlisten: (() => void) | null = null;
    void listen<unknown>(SNAPSHOT_EVENT, (event) => {
      const result = snapshotSchema.safeParse(event.payload);
      // A malformed push is skipped; the next valid one (or a reload) recovers.
      if (result.success) listener(result.data);
    }).then((off) => {
      if (stopped) off();
      else unlisten = off;
    });
    void this.getSnapshot().then((s) => {
      if (!stopped) listener(s);
    });
    return () => {
      stopped = true;
      unlisten?.();
    };
  }

  async hireOptions(): Promise<readonly ToolOptions[]> {
    return parseWire(hireOptionsSchema, "list of hire options", await call("hire_options"));
  }

  async spawnSession(projectId: string, request: HireRequest): Promise<Session> {
    return parseWire(sessionSchema, "session", await call("spawn_session", { projectId, request }));
  }

  async answerQuestion(questionId: string, answer: QuestionAnswer): Promise<void> {
    await call("answer_question", { questionId, answer });
  }

  async tickTodo(todoId: string): Promise<void> {
    await call("tick_todo", { todoId });
  }

  async sendAgentMessage(fromSessionId: string, toSessionId: string, text: string): Promise<void> {
    await call("send_agent_message", { fromSessionId, toSessionId, text });
  }

  async focusSession(sessionId: string): Promise<void> {
    await call("focus_session", { sessionId });
  }

  async addFloor(path: string): Promise<Project> {
    return parseWire(projectSchema, "floor", await call("add_floor", { path }));
  }

  /** macOS's own picker (osascript `choose folder`) via src-tauri/src/focus.rs. */
  readonly canPickFolder = true;

  async pickFolder(): Promise<string | null> {
    return parseWire(folderSchema, "folder", await call("pick_folder"));
  }

  subscribeTerminal(sessionId: string, handlers: TerminalHandlers): () => void {
    let stopped = false;
    let exited = false;
    const unlisten: (() => void)[] = [];
    // Live events that arrive before the backlog has been replayed.
    const early: TerminalLive[] = [];
    let replayed = false;
    let terminalId = "";
    let seq = 0;

    const exit = (code: number | null): void => {
      if (exited) return;
      exited = true;
      handlers.exit(code);
    };
    const deliver = (e: TerminalLive): void => {
      if (e.terminalId !== terminalId) return;
      if (e.kind === "exit") {
        exit(e.exitCode);
      } else if (e.seq > seq) {
        seq = e.seq;
        handlers.output(e.data);
      }
    };
    const receive = (e: TerminalLive): void => {
      if (stopped) return;
      if (replayed) deliver(e);
      else early.push(e);
    };

    // Keep each listener as soon as it attaches, so one failing cannot leak the other.
    const keep = (off: () => void): void => {
      if (stopped) off();
      else unlisten.push(off);
    };

    // Listen first, then read the backlog: `seq` drops what both carry.
    Promise.all([
      listen<unknown>(TERMINAL_OUTPUT_EVENT, (event) => {
        const r = terminalOutputSchema.safeParse(event.payload);
        if (r.success) receive({ kind: "output", ...r.data });
      }).then(keep),
      listen<unknown>(TERMINAL_EXIT_EVENT, (event) => {
        const r = terminalExitSchema.safeParse(event.payload);
        if (r.success) receive({ kind: "exit", ...r.data });
      }).then(keep),
    ])
      .then(async () => {
        if (stopped) return;
        const backlog = parseWire(terminalBacklogSchema, "terminal backlog", await call("terminal_backlog", { sessionId }));
        if (stopped) return;
        terminalId = backlog.terminalId;
        seq = backlog.seq;
        if (backlog.data !== "") handlers.output(backlog.data);
        replayed = true;
        for (const e of early.splice(0)) deliver(e);
        if (!backlog.running) exit(backlog.exitCode);
      })
      .catch((err: unknown) => {
        if (!stopped) handlers.error(asError(err).message);
      });

    return () => {
      stopped = true;
      for (const off of unlisten.splice(0)) off();
    };
  }

  subscribeHireFailures(listener: (failure: HireFailure) => void): () => void {
    return follow(
      (deliver) =>
        listen<unknown>(HIRE_FAILED_EVENT, (event) => {
          const r = hireFailureSchema.safeParse(event.payload);
          if (r.success) deliver(r.data);
        }),
      listener,
    );
  }

  subscribeFileDrops(listener: (drop: FileDrop) => void): () => void {
    return follow(
      (deliver) =>
        getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type !== "drop") return;
          const { x, y } = event.payload.position.toLogical(window.devicePixelRatio);
          deliver({ paths: event.payload.paths, x, y });
        }),
      listener,
    );
  }

  async writeTerminal(sessionId: string, data: string): Promise<void> {
    await call("terminal_write", { sessionId, data });
  }

  async resizeTerminal(sessionId: string, cols: number, rows: number): Promise<void> {
    await call("terminal_resize", { sessionId, cols, rows });
  }
}
