import { useMemo, useRef } from "react";
import type { OfficeSnapshot } from "../types";
import { EMPTY_MEMORY, type ClusterMemory, type Grouping, groupByProject } from "./clusters";

const EMPTY: Grouping = { clusters: [], clusterOf: new Map(), tags: new Map(), unplaced: [] };

/** Memory lives for the app run, so it is never reset when the snapshot changes. */
export function useClusters(snapshot: OfficeSnapshot | null): Grouping {
  const memory = useRef<ClusterMemory>(EMPTY_MEMORY);
  return useMemo(() => {
    if (!snapshot) return EMPTY;
    const { memory: next, ...grouping } = groupByProject(snapshot, memory.current);
    memory.current = next;
    return grouping;
  }, [snapshot]);
}
