import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PlacePanel } from "./PlacePanel";
import { NeedsYou } from "./NeedsYou";
import { ProjectsContext, type ProjectsValue } from "../../state/ProjectsContext";
import { similarNames } from "../../state/similarNames";
import { makeSeedSnapshot } from "../../api/mockData";
import type { OfficeSnapshot, Session } from "../../types";

const base = makeSeedSnapshot();
const rule = (id: string, name: string) => ({ id, name, prefixes: [], contains: [], repos: [] });
const named = (id: string, name: string | null): Session => ({ ...base.sessions[0], id, title: name ?? id, sessionName: name, sessionNameSource: "user" }) as Session;

function value(over: Partial<ProjectsValue> = {}): ProjectsValue {
  return {
    clusters: [],
    clusterOf: new Map(),
    tags: new Map(),
    unplaced: [],
    rules: [rule("alpha", "Alpha"), rule("beta", "Beta"), rule("alpha-fixing", "Alpha fixing")],
    rulesError: null,
    similar: similarNames,
    assignProject: vi.fn(() => Promise.resolve()),
    ...over,
  };
}

function panel(v: ProjectsValue, sessions: Session[] = []) {
  render(
    <ProjectsContext value={v}>
      <PlacePanel projectId="p-one" project={{ ...base.projects[0]!, path: "/x/repo-one" }} name="repo-one" noun="island" enterLabel="Open island" sessions={sessions} counts={{ active: 0, total: sessions.length }} asks={[]} moveTargets={[]} leaving={false} className="arch-panel" onRename={vi.fn()} onMove={vi.fn()} onOpenSession={vi.fn()} onEnter={vi.fn()} />
    </ProjectsContext>,
  );
}

describe("the Project field and pickers", () => {
  it("offers the similar name first and still allows creating the typed one", async () => {
    const v = value();
    panel(v);
    const user = userEvent.setup();
    await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), "New project");
    await user.type(screen.getByRole("textbox", { name: "New project" }), "Alphafix");
    const buttons = screen.getAllByRole("button").map((b) => b.textContent);
    expect(buttons.indexOf("Alpha fixing")).toBeGreaterThanOrEqual(0);
    expect(buttons.indexOf("Alpha fixing")).toBeLessThan(buttons.indexOf("Create Alphafix"));
    await user.click(screen.getByRole("button", { name: "Create Alphafix" }));
    expect(v.assignProject).toHaveBeenCalledTimes(1);
    expect(v.assignProject).toHaveBeenCalledWith({ kind: "repo", key: "repo-one" }, { id: null, name: "Alphafix" });
  });

  it("lists projects with a name like the session's before the others", () => {
    panel(value(), [named("s-1", "alpha-fixing-3")]);
    const options = Array.from(screen.getByRole("combobox", { name: "Project for alpha-fixing-3" }).querySelectorAll("option")).map((o) => o.textContent);
    expect(options.indexOf("Alpha fixing")).toBeGreaterThanOrEqual(0);
    expect(options.indexOf("Alpha fixing")).toBeLessThan(options.indexOf("Beta"));
  });

  it("gives a session picker only to named sessions, one assignment per choice", async () => {
    const v = value();
    panel(v, [named("a", "alpha-fix-2-X"), named("b", null)]);
    expect(screen.queryByRole("combobox", { name: "Project for b" })).not.toBeInTheDocument();
    const user = userEvent.setup();
    const pick = screen.getByRole("combobox", { name: "Project for alpha-fix-2-X" });
    await user.selectOptions(pick, "Beta");
    await user.selectOptions(pick, "Alpha");
    expect(v.assignProject).toHaveBeenCalledTimes(2);
    expect(v.assignProject).toHaveBeenNthCalledWith(1, { kind: "session", key: "alpha-fix-2-X" }, { id: "beta", name: "Beta" });
    expect(v.assignProject).toHaveBeenNthCalledWith(2, { kind: "session", key: "alpha-fix-2-X" }, { id: "alpha", name: "Alpha" });
  });

  it("disables the field and pickers and shows the error when the rules file is bad", () => {
    panel(value({ rulesError: "projects.json is not valid" }), [named("a", "beta-web")]);
    expect(screen.getByRole("combobox", { name: "Project" })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "Project for beta-web" })).toBeDisabled();
    expect(screen.getAllByRole("alert")[0]).toHaveTextContent("projects.json is not valid");
  });

  it("shows a session's tag in its row", () => {
    const tags = new Map([["a", { sessionId: "a", kind: "harness" as const, label: "Omega", home: "Beta" }]]);
    panel(value({ tags }), [named("a", "beta-web")]);
    expect(screen.getByTestId("place-tag")).toHaveTextContent("Omega · Beta");
  });
});

describe("Needs you, which project", () => {
  const open = async (v: ProjectsValue, snapshot: OfficeSnapshot = { ...base, questions: [] }) => {
    render(
      <ProjectsContext value={v}>
        <NeedsYou snapshot={snapshot} onOpen={vi.fn()} onAnswer={vi.fn(() => Promise.resolve())} />
      </ProjectsContext>,
    );
    await userEvent.setup().click(screen.getByTestId("needs-you"));
  };
  const unplaced = [named("a", "solo-thing"), named("b", "delta-notes")];

  it("counts and lists an unplaced session with no asks", async () => {
    await open(value({ unplaced: [unplaced[0] as Session] }));
    expect(screen.getByRole("button", { name: "Needs you 1" })).toBeInTheDocument();
    expect(screen.getByTestId("which-project-row")).toHaveTextContent("Which project?");
    expect(screen.queryByText("Nothing is waiting on you.")).not.toBeInTheDocument();
  });

  it("adds to the count of asks", async () => {
    await open(value({ unplaced: [unplaced[0] as Session] }), { ...base, questions: [base.questions[0]!] });
    expect(screen.getByTestId("needs-you")).toHaveTextContent(/^Needs you [0-9]+$/);
    const asks = screen.queryAllByTestId("needs-you-row").length;
    expect(screen.getByTestId("needs-you")).toHaveTextContent(`Needs you ${asks + 1}`);
  });

  it("persists two answers in a row and removes both rows", async () => {
    const v = value({ unplaced });
    await open(v);
    const user = userEvent.setup();
    await user.selectOptions(screen.getByRole("combobox", { name: "Which project for solo-thing" }), "Alpha");
    await user.selectOptions(await screen.findByRole("combobox", { name: "Which project for delta-notes" }), "Beta");
    expect(v.assignProject).toHaveBeenNthCalledWith(1, { kind: "session", key: "solo-thing" }, { id: "alpha", name: "Alpha" });
    expect(v.assignProject).toHaveBeenNthCalledWith(2, { kind: "session", key: "delta-notes" }, { id: "beta", name: "Beta" });
    expect(await screen.findByText("Nothing is waiting on you.")).toBeInTheDocument();
  });
  const view = (v: ProjectsValue) => (
    <ProjectsContext value={v}>
      <NeedsYou snapshot={{ ...base, questions: [] }} onOpen={vi.fn()} onAnswer={vi.fn(() => Promise.resolve())} />
    </ProjectsContext>
  );

  it("shows a row again when a session that was answered leaves unplaced and comes back", async () => {
    const one = [unplaced[0] as Session];
    const { rerender } = render(view(value({ unplaced: one })));
    const user = userEvent.setup();
    await user.click(screen.getByTestId("needs-you"));
    await user.selectOptions(screen.getByRole("combobox", { name: "Which project for solo-thing" }), "Alpha");
    expect(await screen.findByText("Nothing is waiting on you.")).toBeInTheDocument();
    rerender(view(value({ unplaced: [] })));
    rerender(view(value({ unplaced: one })));
    expect(screen.getByRole("button", { name: "Needs you 1" })).toBeInTheDocument();
    expect(screen.getByTestId("which-project-row")).toBeInTheDocument();
  });

  it("drops an answered row and the count when the data no longer lists the session", async () => {
    const { rerender } = render(view(value({ unplaced })));
    const user = userEvent.setup();
    await user.click(screen.getByTestId("needs-you"));
    await user.selectOptions(screen.getByRole("combobox", { name: "Which project for solo-thing" }), "Alpha");
    rerender(view(value({ unplaced: [unplaced[1] as Session] })));
    expect(screen.getByRole("button", { name: "Needs you 1" })).toBeInTheDocument();
    expect(screen.getAllByTestId("which-project-row")).toHaveLength(1);
    expect(screen.queryByRole("combobox", { name: "Which project for solo-thing" })).toBeNull();
  });

  it("counts two unplaced sessions with one name once, in one row", async () => {
    await open(value({ unplaced: [named("a", "twin"), named("c", "twin")] }));
    expect(screen.getByRole("button", { name: "Needs you 1" })).toBeInTheDocument();
    expect(screen.getAllByTestId("which-project-row")).toHaveLength(1);
  });

  it("chooses by id when a rule and a guessed project share a label", async () => {
    const guess = { id: "gamma", label: "Gamma", kind: "project" as const, source: "guess" as const, repoIds: [] };
    const v = value({ rules: [rule("r-g", "Gamma")], clusters: [guess], unplaced: [unplaced[0] as Session] });
    await open(v);
    await userEvent.setup().selectOptions(screen.getByRole("combobox", { name: "Which project for solo-thing" }), screen.getAllByRole("option", { name: "Gamma" })[1] as HTMLElement);
    expect(v.assignProject).toHaveBeenCalledWith({ kind: "session", key: "solo-thing" }, { id: "gamma", name: "Gamma" });
  });
});
