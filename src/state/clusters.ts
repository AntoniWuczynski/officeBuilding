import type { OfficeSnapshot, ProjectRule, Session } from "../types";
import { type Placement, firstWord, nameKey, placeSession, repoFolder } from "./projectGuess";

/** id: rule id or guess key; null = Ungrouped. */
export interface Cluster { id: string | null; label: string; kind: "project" | "harness" | "ungrouped"; source: "rule" | "guess" | null; repoIds: string[] }
/** Only for sessions whose project differs from their island's cluster, unplaced named sessions, or self-reviewed ones. */
export interface SessionTag { sessionId: string; kind: "project" | "harness" | "unplaced"; label: string; home: string | null }
/** Run-long: placements by repo id and raw registry name, labels of remembered clusters (a vanished rule keeps its name), repo cluster by Project.id, guess keys in first-seen order. */
export interface ClusterMemory { placements: ReadonlyMap<string, Placement>; repoClusters: ReadonlyMap<string, string>; labels: ReadonlyMap<string, string>; order: readonly string[] }
export const EMPTY_MEMORY: ClusterMemory = { placements: new Map(), repoClusters: new Map(), labels: new Map(), order: [] };
export interface Grouping { clusters: Cluster[]; clusterOf: ReadonlyMap<string, Cluster>; tags: ReadonlyMap<string, SessionTag>; unplaced: Session[] }

const titleCase = (key: string): string => key.split("-").filter(Boolean).map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");

export function groupByProject(snapshot: OfficeSnapshot, memory: ClusterMemory): Grouping & { memory: ClusterMemory } {
  const rules = snapshot.projectRules;
  const ruleIds = new Set(rules.map((r) => r.id));
  const harness = rules.find((r) => r.id === snapshot.harnessId) ?? null;
  const repos = snapshot.projects.filter((p) => p.id !== snapshot.setupProjectId);
  const folderOf = new Map(repos.map((p) => [p.id, repoFolder(p)]));
  const live = snapshot.sessions.filter((s) => folderOf.has(s.projectId));

  // Two-session guess counts only user/peer names that no rule placed.
  const liveKeys = new Map<string, number>();
  for (const s of live) {
    if (s.sessionName === null || s.sessionNameSource === "auto") continue;
    if (placeSession(s, "", rules, new Map(), undefined).kind === "rule") continue;
    const w = firstWord(nameKey(s.sessionName));
    liveKeys.set(w, (liveKeys.get(w) ?? 0) + 1);
  }
  // Two same-named sessions in different repos keep separate guesses.
  const pkey = (s: Session) => `${s.projectId}\u0000${s.sessionName}`;
  // While the rules file is unreadable its rules read as empty; remembered clusters must survive that.
  const kept = new Set(snapshot.projectRulesError === null ? [] : [...memory.labels.keys()].filter((id) => !ruleIds.has(id)));
  const named = new Map<string, string>();
  for (const [id, label] of memory.labels) if (kept.has(id)) named.set(label.toLowerCase(), id);
  for (const r of [...rules].reverse()) named.set(r.name.toLowerCase(), r.id);
  // A guess whose title is a rule's name is that rule, so naming a guess leaves one cluster.
  const resolve = (id: string) => (ruleIds.has(id) || kept.has(id) ? id : (named.get(titleCase(id).toLowerCase()) ?? id));
  const placements = new Map<string, Placement>();
  const placed = new Map<string, Placement>();
  for (const s of live) {
    const p = placeSession(s, folderOf.get(s.projectId) ?? "", rules, liveKeys, s.sessionName === null ? undefined : memory.placements.get(pkey(s)));
    placed.set(s.id, p.kind === "guess" ? { kind: "guess", projectId: resolve(p.projectId) } : p);
    if (s.sessionName !== null && p.kind === "guess") placements.set(pkey(s), p);
  }

  const order = [...memory.order];
  const guessKnown = (id: string) => order.includes(id);
  const ruleIndex = (id: string) => rules.findIndex((r) => r.id === id);
  const labelOf = (id: string) => rules.find((r) => r.id === id)?.name ?? memory.labels.get(id) ?? titleCase(id);
  const repoClusters = new Map(memory.repoClusters);
  const repoCluster = new Map<string, string | null>();
  for (const p of repos) {
    const folder = folderOf.get(p.id) ?? "";
    const byRule = rules.find((r: ProjectRule) => r.repos.includes(folder));
    const remembered = resolve(repoClusters.get(p.id) ?? "");
    if (byRule) { repoCluster.set(p.id, byRule.id); continue; }
    if (remembered !== "" && (ruleIds.has(remembered) || kept.has(remembered) || guessKnown(remembered))) {
      repoClusters.set(p.id, remembered);
      repoCluster.set(p.id, remembered);
      continue;
    }
    const counts = new Map<string, number>();
    for (const s of live) {
      const pl = placed.get(s.id);
      if (s.projectId === p.id && (pl?.kind === "rule" || pl?.kind === "guess")) counts.set(pl.projectId, (counts.get(pl.projectId) ?? 0) + 1);
    }
    const rank = (id: string) => (ruleIds.has(id) ? ruleIndex(id) : rules.length);
    const top = [...counts].sort((a, b) => b[1] - a[1] || rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]))[0]?.[0];
    if (top === undefined) { repoCluster.set(p.id, null); continue; }
    repoClusters.set(p.id, top);
    repoCluster.set(p.id, top);
  }
  for (const id of repoCluster.values()) if (id !== null && !ruleIds.has(id) && !kept.has(id) && !guessKnown(id)) order.push(id);

  const clusters: Cluster[] = [];
  const byId = new Map<string | null, Cluster>();
  const used = new Set(repoCluster.values());
  const add = (c: Cluster) => { clusters.push(c); byId.set(c.id, c); };
  for (const r of rules) if (used.has(r.id)) add({ id: r.id, label: r.name, kind: r.id === harness?.id ? "harness" : "project", source: "rule", repoIds: [] });
  for (const id of kept) if (used.has(id)) add({ id, label: memory.labels.get(id) ?? titleCase(id), kind: "project", source: "rule", repoIds: [] });
  for (const id of order) if (used.has(id) && !ruleIds.has(id) && !kept.has(id)) add({ id, label: titleCase(id), kind: "project", source: "guess", repoIds: [] });
  if (used.has(null)) add({ id: null, label: "Ungrouped", kind: "ungrouped", source: null, repoIds: [] });
  const clusterOf = new Map<string, Cluster>();
  for (const p of repos) {
    const c = byId.get(repoCluster.get(p.id) ?? null);
    if (c) { c.repoIds.push(p.id); clusterOf.set(p.id, c); }
  }

  const tags = new Map<string, SessionTag>();
  const unplaced: Session[] = [];
  for (const s of live) {
    const pl = placed.get(s.id);
    const home = clusterOf.get(s.projectId)?.label ?? null;
    const own = pl?.kind === "rule" || pl?.kind === "guess" ? pl.projectId : null;
    if (pl?.kind === "unplaced") unplaced.push(s);
    if (s.selfReviewed && harness && own !== harness.id) {
      tags.set(s.id, { sessionId: s.id, kind: "harness", label: harness.name, home: own === null ? home : labelOf(own) });
    } else if (pl?.kind === "unplaced") {
      tags.set(s.id, { sessionId: s.id, kind: "unplaced", label: "Which project?", home });
    } else if (own !== null && own !== clusterOf.get(s.projectId)?.id) {
      tags.set(s.id, { sessionId: s.id, kind: "project", label: labelOf(own), home });
    }
  }
  const labels = new Map<string, string>();
  for (const id of repoClusters.values()) labels.set(id, labelOf(id));
  return { clusters, clusterOf, tags, unplaced, memory: { placements, repoClusters, labels, order } };
}
