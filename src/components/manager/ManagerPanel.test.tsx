import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeSeedSnapshot } from "../../api/mockData";
import { makeAbsentManager } from "../../api/mockManager";
import type { ManagerFloor, ManagerLogEvent, Project, Session, TodoItem } from "../../types";
import { ManagerContext } from "./ManagerContext";
import type { ManagerActions } from "./ManagerContext";
import { ManagerPanel } from "./ManagerPanel";

const NOW = new Date("2026-09-28T15:45:00Z");
const seed = makeSeedSnapshot();

function found<T>(item: T | undefined): T {
  if (item === undefined) throw new Error("missing seed entry");
  return item;
}

const shop: Project = found(seed.projects.find((p) => p.id === "p-shop"));
const sessions = seed.sessions.filter((s) => s.projectId === "p-shop");
const hired: ManagerFloor = found(seed.managers.find((m) => m.projectId === "p-shop"));
const absent = makeAbsentManager("p-shop");
const todos: TodoItem[] = seed.todos.filter((t) => t.projectId === "p-shop" && t.assignee === "ai");

function actionsFor(over: Partial<ManagerActions> = {}): ManagerActions {
  return { hire: vi.fn(() => Promise.resolve()), setPaused: vi.fn(() => Promise.resolve()), setHiring: vi.fn(() => Promise.resolve()), withdraw: vi.fn(), openSettings: vi.fn(), ...over };
}

interface Show {
  readonly manager?: ManagerFloor | undefined;
  readonly session?: Session | undefined;
  readonly todos?: readonly TodoItem[];
  readonly sessions?: readonly Session[];
  readonly onOpenSession?: (s: Session) => void;
  readonly onOpenTodos?: () => void;
}

function show(actions: ManagerActions | null, o: Show = {}): void {
  render(
    <ManagerContext value={actions}>
      <ManagerPanel
        project={shop}
        manager={"manager" in o ? o.manager : hired}
        session={o.session}
        sessions={o.sessions ?? sessions}
        todos={o.todos ?? todos}
        now={NOW}
        onOpenSession={o.onOpenSession ?? vi.fn()}
        onOpenTodos={o.onOpenTodos ?? vi.fn()}
      />
    </ManagerContext>,
  );
}

describe("ManagerPanel with nobody hired", () => {
  it("explains the job and hires on a click, saying Hiring meanwhile", async () => {
    let finish: () => void = () => {};
    const hire = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
    show(actionsFor({ hire }), { manager: absent });
    expect(screen.getByTestId("manager-status")).toHaveTextContent("Nobody manages this floor. A manager reads the floor's TODO list and desks");
    expect(screen.getByText(/It is a Claude Code session on your plan/)).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("manager-hire"));
    expect(hire).toHaveBeenCalledWith("p-shop");
    expect(screen.getByTestId("manager-hire")).toHaveTextContent("Hiring");
    expect(screen.getByTestId("manager-hire")).toBeDisabled();
    finish();
    expect(await screen.findByText("Hire a manager")).toBeEnabled();
  });

  it("keeps an error under the button", async () => {
    show(actionsFor({ hire: () => Promise.reject(new Error("claude is not installed")) }), { manager: absent });
    await userEvent.click(screen.getByTestId("manager-hire"));
    expect(await screen.findByTestId("manager-hire-error")).toHaveTextContent("claude is not installed");
  });

  it("opens Settings", async () => {
    const actions = actionsFor();
    show(actions, { manager: undefined });
    await userEvent.click(screen.getByTestId("manager-settings"));
    expect(actions.openSettings).toHaveBeenCalled();
  });

  it("lists the earlier manager's log", () => {
    show(null, { manager: { ...absent, log: hired.log } });
    expect(screen.getByTestId("manager-earlier")).toBeInTheDocument();
    expect(screen.getAllByTestId("manager-log-entry")).toHaveLength(hired.log.length);
  });

  it("draws no buttons without a provider", () => {
    show(null, { manager: absent });
    expect(screen.queryByTestId("manager-hire")).not.toBeInTheDocument();
    expect(screen.queryByTestId("manager-settings")).not.toBeInTheDocument();
    expect(screen.queryByTestId("manager-earlier")).not.toBeInTheDocument();
  });
});

describe("ManagerPanel with a manager hired", () => {
  const boss: Session = { ...found(sessions[0]), id: "m-1", title: "floor manager" };
  const bound: ManagerFloor = { ...hired, status: { kind: "hired", sessionId: "m-1", startedAt: "2026-09-28T15:00:00Z", lastCallAt: "2026-09-28T15:37:00Z" } };

  it("says how it is and when its tools last answered, and opens its terminal", async () => {
    const onOpenSession = vi.fn();
    show(actionsFor(), { manager: bound, session: boss, onOpenSession });
    expect(screen.getByTestId("manager-status")).toHaveTextContent("Working. Its tools last answered 8 min ago.");
    await userEvent.click(screen.getByTestId("manager-open"));
    expect(onOpenSession).toHaveBeenCalledWith(boss);
    expect(screen.getByText(/type \/exit in its terminal/)).toBeInTheDocument();
  });

  it("says so when its tools have not answered, and offers no terminal before it is bound", () => {
    show(actionsFor(), { manager: { ...hired, status: { kind: "hired", sessionId: null, startedAt: "2026-09-28T15:00:00Z", lastCallAt: null } } });
    expect(screen.getByTestId("manager-status")).toHaveTextContent("Starting. It has not used the building's tools yet. If that stays, open its terminal.");
    expect(screen.queryByTestId("manager-open")).not.toBeInTheDocument();
  });

  it("flips both switches through its actions", async () => {
    const actions = actionsFor();
    show(actions, { manager: hired });
    await userEvent.click(within(screen.getByTestId("manager-paused")).getByRole("switch", { name: "Paused" }));
    expect(actions.setPaused).toHaveBeenCalledWith("p-shop", true);
    await userEvent.click(within(screen.getByTestId("manager-hiring")).getByRole("switch", { name: "May hire desks" }));
    expect(actions.setHiring).toHaveBeenCalledWith("p-shop", false);
    expect(screen.getByText(/Paused, the manager can read the floor/)).toBeInTheDocument();
  });

  it("shows the caps and the desks with no price", () => {
    show(actionsFor(), { manager: { ...hired, caps: { ...hired.caps, unpricedDesks: 2 } } });
    expect(screen.getByTestId("manager-caps")).toHaveTextContent("1 of 3 desks hired, $1.82 of $20.00 spent");
    expect(screen.getByTestId("manager-unpriced")).toHaveTextContent("2 hired desks have no price data, so they count toward the desk cap only.");
  });

  it("says it in the singular for one desk", () => {
    show(actionsFor(), { manager: { ...hired, caps: { ...hired.caps, unpricedDesks: 1 } } });
    expect(screen.getByTestId("manager-unpriced")).toHaveTextContent("1 hired desk has no price data, so it counts toward the desk cap only.");
  });

  it("draws no switches without a provider", () => {
    show(null, { manager: hired });
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
    expect(screen.queryByTestId("manager-caps")).not.toBeInTheDocument();
  });
});

describe("ManagerPanel lists", () => {
  it("shows the first five open agent TODOs and opens the TODO section", async () => {
    const many: TodoItem[] = Array.from({ length: 7 }, (_, i) => ({ id: `x-${i}`, projectId: "p-shop", text: `job ${i}`, done: i === 0, assignee: "ai", source: { kind: "app" } }));
    const onOpenTodos = vi.fn();
    show(actionsFor(), { todos: many, onOpenTodos });
    const plan = screen.getByTestId("manager-plan");
    expect(within(plan).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["job 1", "job 2", "job 3", "job 4", "job 5"]);
    await userEvent.click(screen.getByTestId("manager-open-todo"));
    expect(onOpenTodos).toHaveBeenCalled();
  });

  it("says when the whiteboard is empty", () => {
    show(actionsFor(), { todos: [] });
    expect(screen.getByText("Nothing on the whiteboard.")).toBeInTheDocument();
  });

  it("shows a waiting note with its desk, its reason and Withdraw", async () => {
    const actions = actionsFor();
    show(actions);
    const note = within(screen.getByTestId("manager-queue")).getByTestId("manager-note");
    expect(note).toHaveTextContent("To “audit checkout”");
    expect(note).toHaveTextContent("When the checkout audit is done, list the files you changed.");
    expect(note).toHaveTextContent("Waiting: it is busy.");
    await userEvent.click(within(note).getByTestId("manager-withdraw"));
    expect(actions.withdraw).toHaveBeenCalledWith("p-shop", "n-2");
  });

  it("names a desk that has left, and shows no Withdraw without a provider", () => {
    show(null, { sessions: [] });
    expect(screen.getByTestId("manager-note")).toHaveTextContent("To a desk that has left");
    expect(screen.queryByTestId("manager-withdraw")).not.toBeInTheDocument();
  });

  it("says when nothing waits", () => {
    show(actionsFor(), { manager: { ...hired, queue: [] } });
    expect(screen.getByText("No notes are waiting.")).toBeInTheDocument();
  });

  it("writes each kind of log entry in words, with what became of a note", () => {
    const events: ManagerLogEvent[] = [
      { kind: "note", noteId: "n-9", toSessionId: "s-1", toTitle: "audit checkout", text: "go", state: "overridden" },
      { kind: "hire", tool: "codex", task: "write tests" },
      { kind: "escalation", title: "Which database?" },
      { kind: "refused", tool: "hire_desk", reason: "3 desks are already hired" },
    ];
    show(actionsFor(), { manager: { ...hired, log: events.map((event, i) => ({ id: `g-${i}`, at: "2026-09-28T15:40:00Z", event })) } });
    const log = within(screen.getByTestId("manager-log"));
    const entries = log.getAllByTestId("manager-log-entry").map((e) => e.textContent);
    expect(entries[0]).toContain("5 min ago");
    expect(entries[0]).toContain("To “audit checkout”: go");
    expect(entries[0]).toContain("Replaced by your message");
    expect(entries[1]).toContain("Hired Codex for “write tests”");
    expect(entries[2]).toContain("Asked you: Which database?");
    expect(entries[3]).toContain("Refused a hire: 3 desks are already hired");
  });

  it("says when the log is empty", () => {
    show(actionsFor(), { manager: { ...hired, log: [] } });
    expect(screen.getByText("The manager has not done anything yet.")).toBeInTheDocument();
  });

  it("shows what the manager wrote as plain text", () => {
    const text = "<b>bold</b> [link](https://example.test)";
    show(actionsFor(), { manager: { ...hired, queue: [{ id: "n-7", toSessionId: "s-1", text, queuedAt: "2026-09-28T15:30:00Z", hold: "paused" }] } });
    expect(screen.getByTestId("manager-note")).toHaveTextContent(text);
    expect(screen.getByTestId("manager-note").querySelector("b + b, a")).toBeNull();
  });
});
