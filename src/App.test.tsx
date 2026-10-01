import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";
import { makeSeedSnapshot } from "./api/mockData";
import type { DiscoveryApi, HireFailure } from "./api/DiscoveryApi";
import type { OfficeSnapshot } from "./types";

/** A MockApi with a working folder picker, as in the desktop app. */
class PickerApi extends MockApi {
  override readonly canPickFolder = true;
  nextPick: () => Promise<string | null> = () => Promise.resolve("/Users/me/code/picked");
  override pickFolder(): Promise<string | null> {
    return this.nextPick();
  }
}

// jsdom has no canvas: stand in for xterm.js with a terminal that prints into
// a <pre> and lets a test type or resize.
const xterm = vi.hoisted(() => {
  interface Disposable {
    dispose(): void;
  }
  class FakeTerminal {
    static last: FakeTerminal | null = null;
    cols = 80;
    rows = 24;
    readonly options: { disableStdin?: boolean } = {};
    disposed = false;
    private pre: HTMLElement | null = null;
    private onDataListener: ((data: string) => void) | null = null;
    private onResizeListener: ((size: { cols: number; rows: number }) => void) | null = null;
    constructor() {
      FakeTerminal.last = this;
    }
    loadAddon(): void {}
    open(el: HTMLElement): void {
      this.pre = document.createElement("pre");
      el.append(this.pre);
    }
    write(data: string): void {
      if (this.pre !== null) this.pre.textContent = `${this.pre.textContent}${data}`;
    }
    onData(listener: (data: string) => void): Disposable {
      this.onDataListener = listener;
      return { dispose: () => (this.onDataListener = null) };
    }
    onResize(listener: (size: { cols: number; rows: number }) => void): Disposable {
      this.onResizeListener = listener;
      return { dispose: () => (this.onResizeListener = null) };
    }
    focus(): void {}
    dispose(): void {
      this.disposed = true;
    }
    type(data: string): void {
      this.onDataListener?.(data);
    }
    resize(cols: number, rows: number): void {
      this.cols = cols;
      this.rows = rows;
      this.onResizeListener?.({ cols, rows });
    }
  }
  class FakeFitAddon {
    fit(): void {}
  }
  return { FakeTerminal, FakeFitAddon };
});
vi.mock("@xterm/xterm", () => ({ Terminal: xterm.FakeTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: xterm.FakeFitAddon }));

function lastTerminal(): InstanceType<typeof xterm.FakeTerminal> {
  const term = xterm.FakeTerminal.last;
  if (term === null) throw new Error("no terminal was opened");
  return term;
}

/** A MockApi whose subscribers can be handed an arbitrary snapshot (an upstream change). */
class PushableApi extends MockApi {
  private readonly pushed = new Set<(snapshot: OfficeSnapshot) => void>();
  override subscribe(listener: (snapshot: OfficeSnapshot) => void): () => void {
    this.pushed.add(listener);
    const off = super.subscribe(listener);
    return () => {
      this.pushed.delete(listener);
      off();
    };
  }
  push(snapshot: OfficeSnapshot): void {
    for (const listener of this.pushed) listener(snapshot);
  }
}

function renderApp(api: DiscoveryApi = new MockApi()): { api: DiscoveryApi } {
  render(<App api={api} />);
  return { api };
}

function storey(projectId: string): HTMLElement {
  const el = screen.getAllByTestId("storey").find((s) => s.getAttribute("data-project") === projectId);
  if (el === undefined) throw new Error(`no storey ${projectId}`);
  return el;
}

async function enter(projectId: string): Promise<void> {
  const user = userEvent.setup();
  await user.click(within(await findStorey(projectId)).getByTestId("floor-nameplate"));
  await screen.findByTestId("back-button");
}

async function findStorey(projectId: string): Promise<HTMLElement> {
  await screen.findAllByTestId("storey");
  return storey(projectId);
}

describe("building", () => {
  it("has a storey per project with its light and tally", async () => {
    renderApp();
    expect(await screen.findAllByTestId("storey")).toHaveLength(4);
    expect(storey("p-shop")).toHaveAttribute("data-light", "red");
    expect(storey("p-notes")).toHaveAttribute("data-light", "off");
    // arcade has one idle agent: nobody working, so its light is off.
    expect(storey("p-arcade")).toHaveAttribute("data-light", "off");
    expect(storey("p-office")).toHaveAttribute("data-light", "green");
    expect(screen.getByTestId("tally")).toHaveTextContent("2/6");
  });

  it("dims floors that do not match the search and says when nothing matches", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findAllByTestId("storey");
    await user.type(screen.getByRole("searchbox", { name: "Search floors" }), "arcade");
    expect(storey("p-arcade")).not.toHaveClass("is-dim");
    expect(storey("p-shop")).toHaveClass("is-dim");
    expect(screen.getAllByTestId("storey")).toHaveLength(4);
    await user.type(screen.getByRole("searchbox", { name: "Search floors" }), "zzz");
    expect(screen.getByText(/No floors match/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear search" }));
    expect(storey("p-shop")).not.toHaveClass("is-dim");
  });

  it("lists floors that need you and goes straight in", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findAllByTestId("storey");
    await user.click(screen.getByRole("button", { name: "corner_shop" }));
    expect(await screen.findByRole("heading", { name: "corner_shop" })).toBeInTheDocument();
  });

  it("adds a floor from the roof, says when it already exists, and explains bad paths", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findAllByTestId("storey");
    await user.click(screen.getByTestId("add-floor"));
    const dialog = screen.getByTestId("add-floor-dialog");
    expect(within(dialog).queryByTestId("pick-folder")).not.toBeInTheDocument();
    expect(screen.getByTestId("add-floor-submit")).toBeDisabled();
    await user.type(within(dialog).getByRole("textbox"), "code/relative");
    await user.click(screen.getByTestId("add-floor-submit"));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Could not add the floor: give the full path");
    await user.clear(within(dialog).getByRole("textbox"));
    await user.type(within(dialog).getByRole("textbox"), "~/code/newapp");
    await user.click(screen.getByTestId("add-floor-submit"));
    expect(await screen.findByText("Added the newapp floor.")).toBeInTheDocument();
    expect(screen.queryByTestId("add-floor-dialog")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("storey")).toHaveLength(5);
    expect(storey(screen.getAllByTestId("storey").find((s) => s.textContent?.includes("newapp"))?.getAttribute("data-project") ?? "")).toHaveAttribute("data-light", "off");
    await user.click(screen.getByTestId("add-floor"));
    await user.type(within(screen.getByTestId("add-floor-dialog")).getByRole("textbox"), "~/code/arcade");
    await user.click(screen.getByTestId("add-floor-submit"));
    expect(await screen.findByText("arcade already has a floor.")).toBeInTheDocument();
    await user.click(screen.getByTestId("add-floor"));
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("add-floor-dialog")).not.toBeInTheDocument();
  });

  it("fills the folder from the native picker where there is one", async () => {
    const user = userEvent.setup();
    const api = new PickerApi();
    renderApp(api);
    await screen.findAllByTestId("storey");
    await user.click(screen.getByTestId("add-floor"));
    await user.click(screen.getByTestId("pick-folder"));
    expect(within(screen.getByTestId("add-floor-dialog")).getByRole("textbox")).toHaveValue("/Users/me/code/picked");
    api.nextPick = () => Promise.reject(new Error("picker failed"));
    await user.click(screen.getByTestId("pick-folder"));
    expect(await screen.findByRole("alert")).toHaveTextContent("picker failed");
  });

  it("shows an empty building with directions", async () => {
    renderApp(new MockApi({ ...makeSeedSnapshot(), projects: [], sessions: [], questions: [] }));
    expect(await screen.findByText(/No projects yet/)).toBeInTheDocument();
  });
});

describe("floor", () => {
  it("enters a floor: a worker per session, the hire desk, the rail and the room objects", async () => {
    renderApp();
    await enter("p-shop");
    expect(screen.getAllByTestId("worker")).toHaveLength(3);
    expect(screen.getByTestId("hire-desk")).toBeInTheDocument();
    for (const id of ["info", "decisions", "human-todo", "todo", "spend"]) {
      expect(screen.getByTestId(`rail-${id}`)).toBeInTheDocument();
    }
    expect(screen.getByTestId("object-decisions")).toHaveTextContent("2 decisions waiting");
  });

  it("hovering a worker shows its card above the scene, not inside it", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-shop");
    const worker = screen.getByRole("button", { name: /audit checkout/ });
    await user.hover(worker);
    const card = screen.getByTestId("worker-card");
    expect(card).toHaveTextContent("claude-opus-5-5, high effort");
    expect(card.closest(".world")).toBeNull();
    await user.unhover(worker);
    expect(screen.queryByTestId("worker-card")).not.toBeInTheDocument();
    act(() => worker.focus());
    expect(screen.getByTestId("worker-card")).toHaveTextContent("audit checkout");
  });

  it("seats a session's helpers at tiny desks beside its own, folding a big team into a count", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-shop");
    const pod = screen.getByTestId("pod");
    expect(within(pod).getAllByTestId("pod-desk")).toHaveLength(3);
    expect(within(pod).queryByTestId("pod-more")).toBeNull();
    const worker = screen.getByRole("button", { name: /audit checkout, Claude Code, Working, 3 helpers working/ });
    await user.hover(worker);
    expect(screen.getByTestId("worker-card")).toHaveTextContent("3 helpers working");

    await user.click(screen.getByTestId("back-button"));
    await enter("p-office");
    const team = screen.getByTestId("pod");
    expect(within(team).getAllByTestId("pod-desk")).toHaveLength(64);
    expect(within(team).getByTestId("pod-more")).toHaveTextContent("+186");
  });

  it("a stuck agent lies on the floor", async () => {
    renderApp();
    await enter("p-shop");
    const stuck = screen.getAllByTestId("worker").find((w) => w.getAttribute("data-state") === "error");
    expect(stuck).toHaveClass("is-down");
    expect(stuck?.querySelector("[data-pose='stuck']")).not.toBeNull();
  });

  it("opens an app-spawned session's terminal next to, not under, the sidebar", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-shop");
    await user.click(screen.getByRole("button", { name: /audit checkout/ }));
    await user.click(screen.getByTestId("rail-decisions"));
    const terminal = screen.getByTestId("terminal-panel");
    const sidebar = screen.getByTestId("sidebar");
    expect(terminal.closest(".floor-main")).not.toBeNull();
    expect(sidebar.closest(".floor-main")).toBeNull();
    const term = lastTerminal();
    const output = within(terminal).getByTestId("terminal-screen");
    expect(output).toHaveTextContent("Demo terminal for “audit checkout”");
    term.type("run the tests\r");
    await waitFor(() => expect(output).toHaveTextContent("echo: run the tests"));
    await user.click(screen.getByTestId("terminal-close"));
    await waitFor(() => expect(screen.queryByTestId("terminal-panel")).not.toBeInTheDocument());
    expect(term.disposed).toBe(true);
  });

  it("reopening a terminal replays what it printed, and tells the backend its size", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const resize = vi.spyOn(api, "resizeTerminal");
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByRole("button", { name: /audit checkout/ }));
    expect(resize).toHaveBeenCalledWith("s-1", 80, 24);
    lastTerminal().resize(120, 30);
    expect(resize).toHaveBeenLastCalledWith("s-1", 120, 30);
    lastTerminal().type("ls\r");
    await user.click(screen.getByTestId("terminal-close"));
    await waitFor(() => expect(screen.queryByTestId("terminal-panel")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: /audit checkout/ }));
    expect(screen.getByTestId("terminal-screen")).toHaveTextContent("echo: ls");
  });

  it("says when the agent in a terminal exits, and stops taking input", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    vi.spyOn(api, "subscribeTerminal").mockImplementation((_id, handlers) => {
      handlers.output("bye");
      handlers.exit(3);
      return () => {};
    });
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByRole("button", { name: /audit checkout/ }));
    expect(within(screen.getByTestId("terminal-panel")).getByRole("status")).toHaveTextContent("The agent exited with code 3. Its last output stays above.");
    expect(lastTerminal().options.disableStdin).toBe(true);
    expect(screen.getByTestId("terminal-screen")).toHaveTextContent("bye");
  });

  it("an agent whose exit code is unknown has simply exited", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    vi.spyOn(api, "subscribeTerminal").mockImplementation((_id, handlers) => {
      handlers.exit(null);
      return () => {};
    });
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByRole("button", { name: /audit checkout/ }));
    expect(within(screen.getByTestId("terminal-panel")).getByRole("status")).toHaveTextContent("The agent has exited.");
  });

  it("reports a terminal it cannot reach or type into", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    vi.spyOn(api, "writeTerminal").mockRejectedValue(new Error("the agent has exited"));
    vi.spyOn(api, "subscribeTerminal").mockImplementation((_id, handlers) => {
      handlers.error("unknown terminal: t-9");
      return () => {};
    });
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByRole("button", { name: /audit checkout/ }));
    expect(screen.getByRole("alert")).toHaveTextContent("Terminal problem: unknown terminal: t-9.");
    lastTerminal().type("x");
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Terminal problem: the agent has exited."));
  });

  it("raises a discovered session's own window instead of opening a terminal", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const focus = vi.spyOn(api, "focusSession");
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByRole("button", { name: /map-tiles mirror/ }));
    expect(focus).toHaveBeenCalledWith("s-3");
    expect(await screen.findByTestId("toast")).toHaveTextContent("Brought the opencode window");
    expect(screen.queryByTestId("terminal-panel")).not.toBeInTheDocument();
  });

  it("reports a window that cannot be raised", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    vi.spyOn(api, "focusSession").mockRejectedValue(new Error("window closed"));
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByRole("button", { name: /map-tiles mirror/ }));
    expect(await screen.findByTestId("toast")).toHaveTextContent("Could not raise the window");
  });

  it("shows a read-only transcript for sessions it cannot control", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-office");
    await user.click(screen.getByRole("button", { name: /character sprites/ }));
    expect(screen.getByTestId("read-only-note")).toBeInTheDocument();
  });

  it("searching desks dims the rest and says when nothing matches", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-shop");
    await user.type(screen.getByRole("searchbox", { name: "Search desks" }), "zzz");
    expect(screen.getByText(/No desks match/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear search" }));
    expect(screen.queryByText(/No desks match/)).not.toBeInTheDocument();
  });

  it("hires an agent onto the floor with a chosen model and effort", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const spawn = vi.spyOn(api, "spawnSession");
    renderApp(api);
    await enter("p-arcade");
    await user.click(screen.getByTestId("hire-desk"));
    const dialog = screen.getByTestId("hire-dialog");
    // Defaults come from the tool's own settings: Opus 5.5 at high.
    expect(await within(dialog).findByTestId("hire-model")).toHaveValue("claude-opus-5-5");
    expect(within(dialog).getByLabelText("High")).toBeChecked();
    await user.click(within(dialog).getByLabelText("Codex"));
    expect(within(dialog).getByTestId("hire-model")).toHaveValue("gpt-6-astra");
    expect(within(dialog).getByLabelText("Extra high")).toBeChecked();
    await user.click(within(dialog).getByLabelText("Ultra"));
    // A model without "ultra" falls back to its own default effort.
    await user.selectOptions(within(dialog).getByTestId("hire-model"), "gpt-5.5");
    expect(within(dialog).queryByLabelText("Ultra")).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("Medium")).toBeChecked();
    // A model that has the current effort keeps it.
    await user.click(within(dialog).getByLabelText("High"));
    await user.selectOptions(within(dialog).getByTestId("hire-model"), "gpt-5.6-luna");
    expect(within(dialog).getByLabelText("High")).toBeChecked();
    expect(within(dialog).getByText("Greater reasoning depth for complex problems.")).toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox"), "port the renderer");
    await user.click(screen.getByTestId("hire-submit"));
    expect(spawn).toHaveBeenCalledWith("p-arcade", { tool: "codex", model: "gpt-5.6-luna", effort: "high", title: "port the renderer" });
    expect(await screen.findByText(/Hired Codex \(gpt-5.6-luna, high effort\) for “port the renderer”/)).toBeInTheDocument();
    // The new hire walks in from the front, then takes the desk.
    const walk = screen.getByTestId("walk");
    expect(screen.getAllByTestId("worker")).toHaveLength(1);
    fireEvent.animationEnd(walk);
    expect(screen.getAllByTestId("worker")).toHaveLength(2);
    expect(screen.queryByTestId("hire-dialog")).not.toBeInTheDocument();
  });

  it("keeps the hire dialog open with a reason when hiring fails, and Escape cancels", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    vi.spyOn(api, "spawnSession").mockRejectedValue(new Error("no seats"));
    renderApp(api);
    await enter("p-arcade");
    await user.click(screen.getByTestId("hire-desk"));
    await screen.findByTestId("hire-model");
    await user.click(screen.getByTestId("hire-submit"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not hire: no seats");
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("hire-dialog")).not.toBeInTheDocument();
  });

  it("says when the hire options cannot load, and retries", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const load = vi.spyOn(api, "hireOptions").mockRejectedValueOnce(new Error("config unreadable"));
    renderApp(api);
    await enter("p-arcade");
    await user.click(screen.getByTestId("hire-desk"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the tools: config unreadable");
    expect(screen.getByTestId("hire-submit")).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByTestId("hire-model")).toBeInTheDocument();
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("shows an empty floor with directions", async () => {
    renderApp();
    await enter("p-notes");
    expect(screen.getByText(/Nobody works on this floor yet/)).toBeInTheDocument();
  });

  it("goes back to the building", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-shop");
    await user.click(screen.getByTestId("back-button"));
    expect(await screen.findAllByTestId("storey")).toHaveLength(4);
  });
});

describe("manager's office", () => {
  it("Decisions: list by priority, open one, answer A, see it decided", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-shop");
    await user.click(screen.getByTestId("object-decisions"));
    const sidebar = screen.getByTestId("sidebar");
    const rows = within(sidebar).getAllByTestId("question-row");
    expect(rows.map((r) => r.getAttribute("data-question"))).toEqual(["q-2", "q-1"]);
    await user.click(rows[1] ?? sidebar);
    expect(within(sidebar).getByTestId("question-detail")).toHaveTextContent("Persist EventType");
    const [optionA] = within(sidebar).getAllByTestId("question-option");
    await user.click(optionA ?? sidebar);
    expect(await within(sidebar).findAllByTestId("question-row")).toHaveLength(1);
    expect(within(sidebar).getByText("Schema bump (Zod, Pydantic and parity)")).toBeInTheDocument();
    expect(await screen.findByText(/“orders ingest fix” is back at work/)).toBeInTheDocument();
  });

  it("Decisions: a hand-started agent's question offers to raise its terminal instead", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const focus = vi.spyOn(api, "focusSession");
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByTestId("rail-decisions"));
    const sidebar = screen.getByTestId("sidebar");
    await user.click(within(sidebar).getAllByTestId("question-row")[0] ?? sidebar);
    expect(within(sidebar).queryAllByTestId("question-option")).toHaveLength(0);
    expect(within(sidebar).getByText("Use the paid endpoint (TILE_ENDPOINTS)")).toBeInTheDocument();
    expect(within(sidebar).getByText("This agent was started outside the app, so answer it in its own terminal.")).toBeInTheDocument();
    await user.click(within(sidebar).getByTestId("question-raise"));
    expect(within(sidebar).getByTestId("question-raise")).toHaveTextContent("Bring its terminal to the front");
    expect(focus).toHaveBeenCalledWith("s-3");
  });

  it("Decisions: an app-owned agent's terminal question opens its in-app terminal", async () => {
    const user = userEvent.setup();
    const seed = makeSeedSnapshot();
    const api = new MockApi({ ...seed, questions: seed.questions.map((q) => (q.id === "q-1" ? { ...q, answerVia: "terminal" as const } : q)) });
    const focus = vi.spyOn(api, "focusSession");
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByTestId("rail-decisions"));
    const sidebar = screen.getByTestId("sidebar");
    await user.click(within(sidebar).getAllByTestId("question-row")[1] ?? sidebar);
    expect(within(sidebar).getByText("This agent runs in the app, so answer it in its terminal.")).toBeInTheDocument();
    await user.click(within(sidebar).getByRole("button", { name: "Open its terminal" }));
    expect(screen.getByTestId("terminal-panel")).toHaveAttribute("data-session", "s-2");
    expect(focus).not.toHaveBeenCalled();
  });

  it("Decisions: answers with free text, and goes back to the list", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-shop");
    await user.click(screen.getByTestId("rail-decisions"));
    const sidebar = screen.getByTestId("sidebar");
    await user.click(within(sidebar).getAllByTestId("question-row")[1] ?? sidebar);
    await user.click(within(sidebar).getByRole("button", { name: "All decisions" }));
    await user.click(within(sidebar).getAllByTestId("question-row")[1] ?? sidebar);
    expect(within(sidebar).getByTestId("question-other-submit")).toBeDisabled();
    await user.type(within(sidebar).getByLabelText("Other"), "prefill only for now");
    await user.click(within(sidebar).getByTestId("question-other-submit"));
    expect(await within(sidebar).findByText("prefill only for now")).toBeInTheDocument();
  });

  it("Decisions: an answer that fails stays open with the reason", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    vi.spyOn(api, "answerQuestion").mockRejectedValue(new Error("agent went away"));
    renderApp(api);
    await enter("p-shop");
    await user.click(screen.getByTestId("rail-decisions"));
    const sidebar = screen.getByTestId("sidebar");
    await user.click(within(sidebar).getAllByTestId("question-row")[1] ?? sidebar);
    await user.click(within(sidebar).getAllByTestId("question-option")[0] ?? sidebar);
    expect(await within(sidebar).findByRole("alert")).toHaveTextContent("Answer not sent: agent went away");
    expect(within(sidebar).getByTestId("question-detail")).toBeInTheDocument();
  });

  it("Decisions is scoped to the floor", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-notes");
    await user.click(screen.getByTestId("rail-decisions"));
    expect(screen.getByTestId("queue-empty")).toBeInTheDocument();
    await user.click(screen.getByTestId("back-button"));
    await enter("p-office");
    await user.click(screen.getByTestId("rail-decisions"));
    expect(screen.getByText("2.5D cutaway building")).toBeInTheDocument();
    expect(screen.queryByText("Checkout audit runs weekly")).not.toBeInTheDocument();
  });

  it("Decisions: an open call from the repo's decisions file shows its context and is answered in the app", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-office");
    await user.click(screen.getByTestId("rail-decisions"));
    const sidebar = screen.getByTestId("sidebar");
    const row = within(sidebar).getByTestId("question-row");
    expect(row).toHaveTextContent("From FOUNDER_DECISIONS.md");
    await user.click(row);
    expect(within(sidebar).getByText(/Repos already keep TODO.md/)).toBeInTheDocument();
    expect(within(sidebar).getByText(/Your answer is written into FOUNDER_DECISIONS.md/)).toBeInTheDocument();
    await user.click(within(sidebar).getAllByTestId("question-option")[0] ?? sidebar);
    expect(await within(sidebar).findByTestId("queue-empty")).toBeInTheDocument();
    expect(within(sidebar).getByText("Read TODO.md, FOUNDER_TODO.md and FOUNDER_DECISIONS.md")).toBeInTheDocument();
  });

  it("Human TODO and TODO tick off, and the rail toggles the panel", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-office");
    await user.click(screen.getByTestId("object-human-todo"));
    expect(screen.getByText("Tick an item off when you have done it.")).toBeInTheDocument();
    const check = screen.getAllByTestId("todo-check").find((c) => c.getAttribute("data-todo") === "t-1");
    await user.click(check ?? document.body);
    expect(check).toBeChecked();
    await user.click(screen.getByTestId("object-todo"));
    expect(screen.getByRole("heading", { name: "TODO" })).toBeInTheDocument();
    await user.click(screen.getByTestId("rail-todo"));
    await waitFor(() => expect(screen.queryByTestId("sidebar")).not.toBeInTheDocument());
  });

  it("long checklists fold done items and cap open ones", async () => {
    const user = userEvent.setup();
    const seed = makeSeedSnapshot();
    const many = Array.from({ length: 205 }, (_, i) => ({
      id: `bulk-${i}`,
      projectId: "p-arcade",
      text: `item ${i}`,
      done: i >= 203,
      assignee: "ai" as const,
      sourceFile: "TODO.md",
    }));
    renderApp(new MockApi({ ...seed, todos: [...seed.todos, ...many] }));
    await enter("p-arcade");
    await user.click(screen.getByTestId("rail-todo"));
    expect(screen.getAllByTestId("todo-check")).toHaveLength(200);
    await user.click(screen.getByRole("button", { name: "Show all 203 open items" }));
    expect(screen.getAllByTestId("todo-check")).toHaveLength(203);
    await user.click(screen.getByTestId("toggle-done"));
    expect(screen.getAllByTestId("todo-check")).toHaveLength(205);
    expect(screen.getByTestId("toggle-done")).toHaveTextContent("Hide 2 done");
  });

  it("TODOs read from a repo file say so", async () => {
    const user = userEvent.setup();
    const seed = makeSeedSnapshot();
    renderApp(new MockApi({ ...seed, todos: seed.todos.map((t) => (t.assignee === "ai" ? { ...t, sourceFile: "TODO.md" } : t)) }));
    await enter("p-office");
    await user.click(screen.getByTestId("rail-todo"));
    expect(screen.getByText("From TODO.md. Ticking an item writes [x] into the file.")).toBeInTheDocument();
  });

  it("Spend is the floor's own and links to sessions; Info describes the floor", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-shop");
    await user.click(screen.getByTestId("object-spend"));
    expect(screen.getByTestId("spend-total")).toHaveTextContent("$2.38");
    await user.click(within(screen.getByTestId("sidebar")).getByRole("button", { name: /audit checkout/ }));
    expect(screen.getByTestId("terminal-panel")).toBeInTheDocument();
    await user.click(screen.getByTestId("object-info"));
    expect(screen.getByText("~/code/corner_shop")).toBeInTheDocument();
    await user.click(screen.getByTestId("sidebar-close"));
    await waitFor(() => expect(screen.queryByTestId("sidebar")).not.toBeInTheDocument());
  });

  it("Spend names tokens from models with no public price", async () => {
    const user = userEvent.setup();
    const seed = makeSeedSnapshot();
    renderApp(new MockApi({ ...seed, sessions: seed.sessions.map((s) => (s.id === "s-1" ? { ...s, spend: { ...s.spend, unpricedTokens: 50_000 } } : s)) }));
    await enter("p-shop");
    await user.click(screen.getByTestId("object-spend"));
    expect(screen.getByText(/at API list prices\. 50k tokens on this floor came from models with no public price/)).toBeInTheDocument();
  });

  it("empty floors say so in every panel", async () => {
    const user = userEvent.setup();
    renderApp();
    await enter("p-notes");
    await user.click(screen.getByTestId("rail-human-todo"));
    expect(screen.getByText("Nothing for you to do on this floor.")).toBeInTheDocument();
    await user.click(screen.getByTestId("rail-todo"));
    expect(screen.getByText("The agents have nothing queued.")).toBeInTheDocument();
    await user.click(screen.getByTestId("rail-spend"));
    expect(screen.getByText("No agents, no spend.")).toBeInTheDocument();
    await user.click(screen.getByTestId("rail-info"));
    expect(screen.getByText("None yet")).toBeInTheDocument();
    await user.click(screen.getByTestId("rail-decisions"));
    expect(screen.getByText("No decisions recorded on this floor yet.")).toBeInTheDocument();
  });
});

describe("live updates and failures", () => {
  it("a note between colleagues walks across the floor", async () => {
    const api = new MockApi();
    renderApp(api);
    await enter("p-shop");
    await act(async () => {
      await api.sendAgentMessage("s-1", "s-2", "rebase onto mine");
    });
    expect(screen.getByTestId("walk")).toBeInTheDocument();
    expect(screen.getAllByTestId("worker")).toHaveLength(2);
  });

  it("returns to the building if the floor disappears", async () => {
    const api = new PushableApi();
    renderApp(api);
    await enter("p-shop");
    const seed = makeSeedSnapshot();
    act(() => {
      api.push({ ...seed, projects: seed.projects.filter((p) => p.id !== "p-shop") });
    });
    expect(await screen.findAllByTestId("storey")).toHaveLength(3);
  });

  it("shows the backend error with a retry", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const load = vi.spyOn(api, "getSnapshot").mockRejectedValueOnce(new Error("socket closed"));
    renderApp(api);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not reach the session backend: socket closed");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findAllByTestId("storey")).toHaveLength(4);
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("reports a TODO that cannot be ticked", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    vi.spyOn(api, "tickTodo").mockRejectedValue(new Error("stale item"));
    renderApp(api);
    await enter("p-office");
    await user.click(screen.getByTestId("rail-human-todo"));
    await user.click(screen.getAllByTestId("todo-check")[0] ?? document.body);
    expect(await screen.findByTestId("toast")).toHaveTextContent("Could not tick that off: stale item");
  });

  it("says when a hired agent stops before it reaches its desk", async () => {
    const api = new MockApi();
    let report: ((failure: HireFailure) => void) | null = null;
    const off = vi.fn();
    vi.spyOn(api, "subscribeHireFailures").mockImplementation((listener) => {
      report = listener;
      return off;
    });
    const { unmount } = render(<App api={api} />);
    await screen.findAllByTestId("storey");
    act(() => report?.({ title: "fix the tests", exitCode: 127, lastLines: ["zsh: command not found: claude"] }));
    expect(await screen.findByTestId("toast")).toHaveTextContent(
      "“fix the tests” stopped before it reached its desk (exit code 127): zsh: command not found: claude",
    );
    act(() => report?.({ title: "quiet", exitCode: null, lastLines: [] }));
    expect(screen.getAllByTestId("toast").at(-1)).toHaveTextContent(/^“quiet” stopped before it reached its desk/);
    unmount();
    expect(off).toHaveBeenCalledTimes(1);
  });
});
