import type { LinearLink, LinearTeam, TodoItem } from "../types";

/** The teams the demo key can see. */
export const MOCK_TEAMS: readonly LinearTeam[] = [
  { id: "team-eng", key: "ENG", name: "Engineering", projects: [{ id: "proj-checkout", name: "Checkout" }, { id: "proj-tiles", name: "Map tiles" }] },
  { id: "team-ops", key: "OPS", name: "Operations", projects: [] },
];

/** Two demo issues for a linked floor: one for the human, one for the agents. */
export function mockLinearTodos(projectId: string, link: LinearLink): TodoItem[] {
  const issue = (n: number, text: string, assignee: TodoItem["assignee"]): TodoItem => {
    const identifier = `${link.teamKey}-${100 + n}`;
    return {
      id: `${projectId}|linear|issue-${n}`,
      projectId,
      text,
      done: false,
      assignee,
      source: { kind: "linear", identifier, url: `https://linear.app/example/issue/${identifier}` },
    };
  };
  return [issue(1, "Check the refund email copy", "human"), issue(2, "Add a retry to the orders ingest", "ai")];
}
