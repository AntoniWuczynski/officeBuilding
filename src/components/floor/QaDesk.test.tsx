import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FloorView } from "./FloorView";
import { makeSeedSnapshot } from "../../api/mockData";
import { MockApi } from "../../api/mockApi";
import { roomDeskSlot } from "../../lib/layout";
import type { OfficeSnapshot, QaFloor } from "../../types";
import type { Section } from "./sections";

const api = new MockApi();

function floor(snapshot: OfficeSnapshot, projectId: string, onSection: (s: Section | null) => void = () => {}): React.ReactElement {
  const project = snapshot.projects.find((p) => p.id === projectId);
  if (project === undefined) throw new Error(`no project ${projectId}`);
  return (
    <FloorView
      project={project}
      snapshot={snapshot}
      now={new Date("2026-09-28T15:45:00Z")}
      section={null}
      openSessionId={null}
      onSection={onSection}
      onOpenSettings={() => {}}
      onOpenPlate={() => {}}
      onOpenSession={() => {}}
      onCloseTerminal={() => {}}
      onBack={() => {}}
      loadHireOptions={() => Promise.resolve([])}
      onHire={() => Promise.resolve()}
      onAnswer={() => Promise.resolve()}
      onTick={() => {}}
      onResume={() => Promise.resolve()}
      onOpenShell={() => {}}
      onNewShell={() => Promise.resolve()}
      onEndShell={() => Promise.resolve()}
      setup={{
        load: () => api.setupView(),
        subscribe: (l) => api.subscribeSetupChanges(l),
        onSetTargets: (k, n, t) => api.setSetupTargets(k, n, t),
        onImport: (f, k, n) => api.importSetupItem(f, k, n),
      }}
    />
  );
}

function withQa(snapshot: OfficeSnapshot, projectId: string, change: (q: QaFloor) => QaFloor): OfficeSnapshot {
  return { ...snapshot, qa: snapshot.qa.map((q) => (q.projectId === projectId ? change(q) : q)) };
}

describe("the QA desk in the corner", () => {
  it("stands on a floor with QA on, with its glass, stamp and the reviewer seated once", () => {
    render(floor(makeSeedSnapshot(), "p-shop"));
    expect(screen.getByTestId("object-qa")).toHaveTextContent("QA");
    expect(screen.getByTestId("qa-stamp")).toHaveTextContent("QA notes");
    expect(screen.getByTestId("desk-glass")).toBeInTheDocument();
    expect(screen.getAllByTestId("worker").filter((w) => w.getAttribute("data-session") === "s-1")).toHaveLength(1);
    expect(screen.getAllByTestId("worker")).toHaveLength(3);
    expect(screen.getByTestId("object-qa")).toHaveAccessibleName(/^QA desk: Reviewing #14 .* QA found problems\. Open Info\.$/);
  });

  it("puts the hire desk where it would be with one session fewer", () => {
    render(floor(makeSeedSnapshot(), "p-shop"));
    // Three sessions on the floor, one of them the reviewer: two ordinary desks, so the hire desk is the third.
    const slot = roomDeskSlot(2);
    const billboard = screen.getByTestId("hire-desk").parentElement;
    expect(billboard).toHaveStyle({ left: `${slot.x + 18}px`, top: `${slot.y}px` });
  });

  it("is not there when QA is off", () => {
    render(floor(makeSeedSnapshot(), "p-notes"));
    expect(screen.queryByTestId("object-qa")).not.toBeInTheDocument();
    expect(screen.queryByTestId("desk-glass")).not.toBeInTheDocument();
  });

  it("draws the bare desk with no stamp when nobody sits there and nothing is stamped", () => {
    const seed = withQa(makeSeedSnapshot(), "p-shop", (q) => ({ ...q, run: null, stamp: null }));
    render(floor(seed, "p-shop"));
    expect(screen.getByTestId("object-qa")).toBeInTheDocument();
    expect(screen.queryByTestId("qa-stamp")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("worker")).toHaveLength(3);
  });

  it("opens the Info panel when its tag is clicked", async () => {
    const opened: (Section | null)[] = [];
    render(floor(makeSeedSnapshot(), "p-shop", (s) => opened.push(s)));
    await userEvent.click(screen.getByTestId("object-qa"));
    expect(opened).toEqual(["info"]);
  });
});
