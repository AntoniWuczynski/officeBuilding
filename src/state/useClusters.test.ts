import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { OfficeSnapshot, Session } from "../types";
import { useClusters } from "./useClusters";

const sess = (id: string, name: string) => ({ id, projectId: "id-repo-one", sessionName: name, sessionNameSource: "user", selfReviewed: false }) as Session;
const snap = (sessions: Session[]) =>
  ({ projects: [{ id: "id-repo-one", name: "n", path: "/x/repo-one", sessionIds: [] }], sessions, projectRules: [], harnessId: null, projectRulesError: null, setupProjectId: "id-setup" }) as unknown as OfficeSnapshot;

describe("useClusters", () => {
  it("returns an empty grouping for a null snapshot", () => {
    const { result } = renderHook(() => useClusters(null));
    expect(result.current.clusters).toEqual([]);
  });
  it("keeps memory across a rerender that drops the partner session", () => {
    const { result, rerender } = renderHook(({ s }) => useClusters(s), { initialProps: { s: snap([sess("1", "beta-web"), sess("2", "beta-api")]) } });
    expect(result.current.clusters.map((c) => c.label)).toEqual(["Beta"]);
    rerender({ s: snap([sess("1", "beta-web")]) });
    expect(result.current.clusters.map((c) => c.label)).toEqual(["Beta"]);
    expect(result.current.unplaced).toEqual([]);
  });
});
