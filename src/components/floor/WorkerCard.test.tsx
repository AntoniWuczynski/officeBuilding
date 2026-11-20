import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { WorkerCard } from "./WorkerCard";
import { ProjectsContext } from "../../state/ProjectsContext";
import { makeSeedSnapshot } from "../../api/mockData";
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
