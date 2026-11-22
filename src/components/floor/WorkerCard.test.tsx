import { describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { WorkerCard } from "./WorkerCard";
import { ProjectsContext } from "../../state/ProjectsContext";
import { makeSeedSnapshot } from "../../api/mockData";
import { UltracodeContext } from "../../state/UltracodeContext";
import type { QueuedNote, Session } from "../../types";
import type { SessionTag } from "../../state/clusters";

const session = makeSeedSnapshot().sessions[0]!;
function card(tag: SessionTag | null) {
  render(
    <ProjectsContext value={{ clusters: [], clusterOf: new Map(), tags: new Map(tag === null ? [] : [[session.id, tag]]), unplaced: [], rules: [], rulesError: null, similar: () => [], assignProject: () => Promise.resolve() }}>
      <WorkerCard session={session} x={0} y={0} now={new Date("2026-09-28T16:00:00Z")} editorHint={null} managed={false} waiting={[]} handover={undefined} />
    </ProjectsContext>,
  );
}

describe("the worker card's project tag", () => {
  it("shows another project's name", () => {
    card({ sessionId: session.id, kind: "project", label: "Beta", home: null });
    expect(screen.getByTestId("worker-tag")).toHaveTextContent("Beta");
  });
  it("shows the Harness label with its home", () => {
    card({ sessionId: session.id, kind: "harness", label: "Omega", home: "Alpha" });
    expect(screen.getByTestId("worker-tag")).toHaveTextContent("Omega · Alpha");
  });
  it("shows no tag when there is none", () => {
    card(null);
    expect(screen.queryByTestId("worker-tag")).not.toBeInTheDocument();
  });
});

describe("the worker card's waiting note", () => {
  it("uses the Codex sentence for a Codex desk with no idle signal", () => {
    const codex = { ...session, tool: "codex" as const };
    const waiting: QueuedNote[] = [{ id: "n-1", toSessionId: codex.id, text: "hi", queuedAt: "2026-09-28T15:36:00Z", hold: "no-idle-signal" }];
    render(
      <ProjectsContext value={{ clusters: [], clusterOf: new Map(), tags: new Map(), unplaced: [], rules: [], rulesError: null, similar: () => [], assignProject: () => Promise.resolve() }}>
        <WorkerCard session={codex} x={0} y={0} now={new Date("2026-09-28T16:00:00Z")} editorHint={null} managed={false} waiting={waiting} handover={undefined} />
      </ProjectsContext>,
    );
    expect(screen.getByTestId("worker-card")).toHaveTextContent("Type /hooks in its terminal and trust them once");
  });
});

describe("the worker card's take-over line", () => {
  const line = (takeOver?: string) =>
    render(
      <ProjectsContext value={{ clusters: [], clusterOf: new Map(), tags: new Map(), unplaced: [], rules: [], rulesError: null, similar: () => [], assignProject: () => Promise.resolve() }}>
        <WorkerCard session={session} x={0} y={0} now={new Date("2026-09-28T16:00:00Z")} editorHint={null} managed={false} waiting={[]} handover={undefined} takeOver={takeOver} />
      </ProjectsContext>,
    );
  it("shows the line it is given", () => {
    line("Took over from the Claude Code session abc at 10:00. That session was not ended.");
    expect(screen.getByTestId("worker-take-over")).toHaveTextContent("Took over from the Claude Code session abc");
  });
  it("shows nothing without one", () => {
    line();
    expect(screen.queryByTestId("worker-take-over")).not.toBeInTheDocument();
  });
});

describe("the worker card's attention lines", () => {
  const draw = (over: Partial<typeof session>, unseen: boolean): void => {
    render(
      <ProjectsContext value={{ clusters: [], clusterOf: new Map(), tags: new Map(), unplaced: [], rules: [], rulesError: null, similar: () => [], assignProject: () => Promise.resolve() }}>
        <WorkerCard session={{ ...session, ...over }} x={0} y={0} now={new Date("2026-09-28T16:00:00Z")} editorHint={null} managed={false} waiting={[]} handover={undefined} unseen={unseen} />
      </ProjectsContext>,
    );
  };

  it("says a working desk has a question for you", () => {
    draw({ state: "running", pendingQuestionIds: ["q"] }, false);
    expect(screen.getByTestId("worker-asks")).toHaveTextContent("Has a question for you");
    expect(screen.queryByTestId("worker-unseen")).toBeNull();
  });

  it("says when approval prompts cannot be seen", () => {
    draw({ state: "idle", control: "raise-window" }, true);
    expect(screen.getByTestId("worker-unseen")).toHaveTextContent("started outside the app");
    expect(screen.queryByTestId("worker-asks")).toBeNull();
  });
});

describe("the worker card's words and face", () => {
  const empty = { clusters: [], clusterOf: new Map(), tags: new Map(), unplaced: [], rules: [], rulesError: null, similar: () => [], assignProject: () => Promise.resolve() };
  const show = (s: Session, ultracode: readonly string[] = []) =>
    render(
      <ProjectsContext value={empty}>
        <UltracodeContext value={new Set(ultracode)}>
          <WorkerCard session={s} x={0} y={0} now={new Date("2026-09-28T16:00:00Z")} editorHint={null} managed={false} waiting={[]} handover={undefined} />
        </UltracodeContext>
      </ProjectsContext>,
    );
  it("shows the model line and the face of the worker its model draws", () => {
    show({ ...session, model: "claude-sonnet-5-5", effort: "high" });
    expect(screen.getByTestId("worker-card")).toHaveTextContent("claude-sonnet-5-5, high effort");
    expect(screen.getByTestId("worker-face")).toHaveAttribute("data-look", "nell-high");
  });
  it("says ultracode only for a Claude Code desk in the context's set", () => {
    show(session, [session.id]);
    expect(screen.getByTestId("worker-ultracode")).toHaveTextContent("Hired with ultracode on");
  });
  it("says nothing of ultracode for a desk outside the set or a Codex desk", () => {
    show(session);
    expect(screen.queryByTestId("worker-ultracode")).not.toBeInTheDocument();
    cleanup();
    show({ ...session, tool: "codex" }, [session.id]);
    expect(screen.queryByTestId("worker-ultracode")).not.toBeInTheDocument();
  });
  it("shows what the helpers run on", () => {
    show({ ...session, helpers: [{ id: "h", state: "running", worktree: false, branch: null, model: "gpt-5.6-terra", effort: "medium" }] });
    expect(screen.getByTestId("worker-helper-models")).toHaveTextContent("Helpers on gpt-5.6-terra, medium effort");
  });
});
