import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { WorkerCard } from "./WorkerCard";
import { ProjectsContext } from "../../state/ProjectsContext";
import { makeSeedSnapshot } from "../../api/mockData";
import type { QueuedNote } from "../../types";
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
