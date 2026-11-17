import { describe, expect, it } from "vitest";
import type { OfficeSnapshot, ProjectRule, Session } from "../types";
import { EMPTY_MEMORY, groupByProject } from "./clusters";

const rule = (id: string, name: string, p: Partial<ProjectRule> = {}): ProjectRule => ({ id, name, prefixes: [], contains: [], repos: [], ...p });
const sess = (id: string, projectId: string, name: string | null, o: Partial<Session> = {}): Session =>
  ({ id, projectId, sessionName: name, sessionNameSource: name === null ? null : "user", selfReviewed: false, ...o }) as Session;
const snap = (repos: string[], sessions: Session[], projectRules: ProjectRule[] = [], o: Partial<OfficeSnapshot> = {}): OfficeSnapshot =>
  ({
    projects: repos.map((r) => ({ id: `id-${r}`, name: `Island ${r}`, path: `/x/${r}`, sessionIds: [] })),
    sessions, projectRules, harnessId: null, projectRulesError: null, setupProjectId: "id-setup", ...o,
  }) as unknown as OfficeSnapshot;

const alpha = rule("alpha", "Alpha", { prefixes: ["alpha-"] });
const beta = rule("beta", "Beta", { prefixes: ["beta-"] });
const labels = (g: ReturnType<typeof groupByProject>) => g.clusters.map((c) => c.label);

describe("groupByProject repo cluster", () => {
  const three = [sess("a1", "id-repo-one", "alpha-fix-2-X"), sess("a2", "id-repo-one", "alpha-two"), sess("b1", "id-repo-one", "beta-web")];
  it("takes the majority at first sight and tags the odd one out", () => {
    const g = groupByProject(snap(["repo-one"], three, [alpha, beta]), EMPTY_MEMORY);
    expect(g.clusterOf.get("id-repo-one")?.label).toBe("Alpha");
    expect(g.tags.get("b1")).toMatchObject({ kind: "project", label: "Beta" });
    expect(g.tags.has("a1")).toBe(false);
  });
  it("a repo rule beats the majority", () => {
    const r = rule("beta", "Beta", { prefixes: ["beta-"], repos: ["repo-one"] });
    expect(groupByProject(snap(["repo-one"], three, [alpha, r]), EMPTY_MEMORY).clusterOf.get("id-repo-one")?.id).toBe("beta");
  });
  it("a tie goes to the earlier rule", () => {
    const two = three.slice(1);
    expect(groupByProject(snap(["repo-one"], two, [beta, alpha]), EMPTY_MEMORY).clusterOf.get("id-repo-one")?.id).toBe("beta");
  });
  it("stays put after the majority sessions end", () => {
    const s1 = groupByProject(snap(["repo-one"], three, [alpha, beta]), EMPTY_MEMORY);
    const s2 = groupByProject(snap(["repo-one"], three.slice(2), [alpha, beta]), s1.memory);
    expect(s2.clusterOf.get("id-repo-one")?.id).toBe("alpha");
    expect(groupByProject(snap(["repo-one"], three.slice(2), [alpha, beta]), EMPTY_MEMORY).clusterOf.get("id-repo-one")?.id).toBe("beta");
  });
  it("a repo with only unplaced or unnamed sessions is Ungrouped until one is placed", () => {
    const lone = [sess("u", "id-repo-one", "solo-thing"), sess("n", "id-repo-one", null)];
    const g1 = groupByProject(snap(["repo-one"], lone, [alpha]), EMPTY_MEMORY);
    expect(g1.clusterOf.get("id-repo-one")?.kind).toBe("ungrouped");
    const g2 = groupByProject(snap(["repo-one"], [...lone, sess("a", "id-repo-one", "alpha-x")], [alpha]), g1.memory);
    expect(g2.clusterOf.get("id-repo-one")?.id).toBe("alpha");
  });
  it("never clusters the setup repo and omits empty clusters", () => {
    const g = groupByProject(snap(["setup", "repo-one"], [sess("a", "id-setup", "alpha-x")], [alpha, beta]), EMPTY_MEMORY);
    expect(g.clusterOf.has("id-setup")).toBe(false);
    expect(labels(g)).toEqual(["Ungrouped"]);
  });
});

describe("groupByProject stability", () => {
  it("keeps a guess when the partner leaves, even under a new session id", () => {
    const web = sess("w1", "id-repo-one", "beta-web"), api = sess("p1", "id-repo-one", "beta-api");
    const g1 = groupByProject(snap(["repo-one"], [web, api]), EMPTY_MEMORY);
    expect(labels(g1)).toEqual(["Beta"]);
    const g2 = groupByProject(snap(["repo-one"], [{ ...web, id: "w2" }]), g1.memory);
    expect(labels(g2)).toEqual(["Beta"]);
    expect(g2.unplaced).toEqual([]);
  });
  it("same input twice gives the same output", () => {
    const s = snap(["repo-one", "repo-two"], [sess("g", "id-repo-two", "gamma-3-X"), sess("g2", "id-repo-two", "gamma-4"), sess("b", "id-repo-one", "beta-1"), sess("b2", "id-repo-one", "beta-2")]);
    const g1 = groupByProject(s, EMPTY_MEMORY);
    const g2 = groupByProject(s, g1.memory);
    expect(labels(g2)).toEqual(labels(g1));
    expect(g2.tags).toEqual(g1.tags);
  });
  it("keeps cluster order when a guess first seen later sorts earlier", () => {
    const zed = [sess("z1", "id-repo-one", "zed-1"), sess("z2", "id-repo-one", "zed-2")];
    const abe = [sess("e1", "id-repo-two", "abe-1"), sess("e2", "id-repo-two", "abe-2")];
    const g1 = groupByProject(snap(["repo-one", "repo-two"], zed), EMPTY_MEMORY);
    const g2 = groupByProject(snap(["repo-one", "repo-two"], [...zed, ...abe]), g1.memory);
    expect(labels(g2)).toEqual(["Zed", "Abe"]);
    expect(labels(groupByProject(snap(["repo-one", "repo-two"], abe), g2.memory))).toEqual(["Zed", "Abe"]);
  });
  it("a sticky guess renamed to a lone name is unplaced", () => {
    const web = sess("w", "id-repo-one", "beta-web");
    const g1 = groupByProject(snap(["repo-one"], [web, sess("p", "id-repo-one", "beta-api")]), EMPTY_MEMORY);
    const g2 = groupByProject(snap(["repo-one"], [web]), g1.memory);
    expect(g2.unplaced).toEqual([]);
    const g3 = groupByProject(snap(["repo-one"], [{ ...web, sessionName: "solo-thing" }]), g2.memory);
    expect(g3.unplaced.map((s) => s.id)).toEqual(["w"]);
    expect(g3.tags.get("w")).toMatchObject({ kind: "unplaced" });
  });
  it("keeps same-named sessions in different repos apart", () => {
    const b = sess("b", "id-repo-two", "beta-web");
    const pair = [sess("a", "id-repo-one", "beta-web"), sess("a2", "id-repo-one", "beta-api")];
    const g1 = groupByProject(snap(["repo-one", "repo-two"], pair), EMPTY_MEMORY);
    const g2 = groupByProject(snap(["repo-one", "repo-two"], [b]), g1.memory);
    expect(g2.unplaced.map((s) => s.id)).toEqual(["b"]);
  });
});

describe("groupByProject rules and names", () => {
  const sessions = [sess("a1", "id-repo-one", "alpha-1"), sess("a2", "id-repo-one", "alpha-2"), sess("a3", "id-repo-one", "alpha-3")];
  const r9 = rule("r-9", "Alpha", { prefixes: ["alpha-"] });
  it("keeps an island's remembered cluster and label while the rules file is unreadable", () => {
    const g1 = groupByProject(snap(["repo-one"], sessions, [r9]), EMPTY_MEMORY);
    const g2 = groupByProject(snap(["repo-one"], sessions, [], { projectRulesError: "bad" }), g1.memory);
    const g3 = groupByProject(snap(["repo-one"], sessions, [r9]), g2.memory);
    for (const g of [g1, g2, g3]) expect(g.clusterOf.get("id-repo-one")).toMatchObject({ id: "r-9", label: "Alpha" });
    expect(g2.tags.size).toBe(0);
    expect(g2.clusters).toHaveLength(1);
  });
  it("re-derives when a remembered rule is deleted without an error", () => {
    const g1 = groupByProject(snap(["repo-one"], sessions, [r9]), EMPTY_MEMORY);
    const g2 = groupByProject(snap(["repo-one"], sessions, []), g1.memory);
    expect(g2.clusterOf.get("id-repo-one")?.id).toBe("alpha");
  });
  it("maps a remembered guess to the rule later named the same", () => {
    const pair = [sess("w", "id-repo-one", "beta-web"), sess("p", "id-repo-one", "beta-api")];
    const g1 = groupByProject(snap(["repo-one"], pair), EMPTY_MEMORY);
    const g2 = groupByProject(snap(["repo-one"], pair, [rule("r-2", "beta")]), g1.memory);
    expect(g2.clusters.map((c) => c.id)).toEqual(["r-2"]);
    expect(g2.tags.size).toBe(0);
  });
  it("puts no harness tag on a session whose own project is the harness", () => {
    const omega = rule("omega", "Omega", { prefixes: ["omega-"] });
    const g = groupByProject(snap(["repo-one"], [sess("o", "id-repo-one", "omega-1", { selfReviewed: true }), sess("a", "id-repo-one", "alpha-x")], [alpha, omega], { harnessId: "omega" }), EMPTY_MEMORY);
    expect(g.tags.get("o")).toMatchObject({ kind: "project", label: "Omega" });
  });
  it("sends an unruled partner of a ruled name to unplaced", () => {
    const r = rule("br", "Beta research", { prefixes: ["beta-web"] });
    const g = groupByProject(snap(["repo-one"], [sess("w", "id-repo-one", "beta-web"), sess("p", "id-repo-one", "beta-api")], [r]), EMPTY_MEMORY);
    expect(g.unplaced.map((s) => s.id)).toEqual(["p"]);
    expect(g.clusters.map((c) => c.id)).toEqual(["br"]);
  });
  it("never invents a project from two auto names", () => {
    const a = sess("x", "id-repo-one", "fix-login-a", { sessionNameSource: "auto" }), b = sess("y", "id-repo-one", "fix-tests-b", { sessionNameSource: "auto" });
    const g = groupByProject(snap(["repo-one"], [a, b]), EMPTY_MEMORY);
    expect(g.clusters.map((c) => c.id)).toEqual([null]);
    expect(g.unplaced).toEqual([]);
  });
  it("placement ignores Project.name", () => {
    const s = snap(["repo-one"], [sess("w", "id-repo-one", "repo-one-lane")]);
    const renamed = { ...s, projects: s.projects.map((p) => ({ ...p, name: "Renamed island" })) } as OfficeSnapshot;
    expect(groupByProject(renamed, EMPTY_MEMORY).clusterOf.get("id-repo-one")?.id).toBe("repo-one");
  });
});

describe("groupByProject tags", () => {
  it("leaves unnamed sessions untagged and unplaced named ones asked", () => {
    const g = groupByProject(snap(["repo-one"], [sess("n", "id-repo-one", null), sess("u", "id-repo-one", "solo-thing")], [alpha]), EMPTY_MEMORY);
    expect(g.tags.has("n")).toBe(false);
    expect(g.tags.get("u")).toMatchObject({ kind: "unplaced", label: "Which project?" });
    expect(g.unplaced.map((s) => s.id)).toEqual(["u"]);
  });
  it("leaves auto-named unguessed sessions untagged and not asked", () => {
    const auto = sess("x", "id-repo-one", "fix-login-a", { sessionNameSource: "auto" });
    const g = groupByProject(snap(["repo-one"], [auto, sess("a", "id-repo-one", "alpha-x")], [alpha]), EMPTY_MEMORY);
    expect(g.tags.has("x")).toBe(false);
    expect(g.unplaced).toEqual([]);
  });
  it("tags a self-reviewed session with the harness and keeps its island", () => {
    const omega = rule("omega", "Omega");
    const sessions = [sess("a1", "id-repo-one", "alpha-fix-2-X", { selfReviewed: true }), sess("a2", "id-repo-one", "alpha-two")];
    const g = groupByProject(snap(["repo-one"], sessions, [alpha, omega], { harnessId: "omega" }), EMPTY_MEMORY);
    expect(g.tags.get("a1")).toEqual({ sessionId: "a1", kind: "harness", label: "Omega", home: "Alpha" });
    expect(g.clusterOf.get("id-repo-one")?.label).toBe("Alpha");
    const off = groupByProject(snap(["repo-one"], sessions, [alpha, omega], { harnessId: null }), EMPTY_MEMORY);
    expect(off.tags.has("a1")).toBe(false);
  });
  it("with no names or rules gives one Ungrouped cluster and no tags", () => {
    const g = groupByProject(snap(["setup", "repo-one", "repo-two"], [sess("n", "id-repo-one", null)]), EMPTY_MEMORY);
    expect(g.clusters).toHaveLength(1);
    expect(g.clusters[0]).toMatchObject({ kind: "ungrouped", id: null, repoIds: ["id-repo-one", "id-repo-two"] });
    expect(g.tags.size).toBe(0);
  });
});
