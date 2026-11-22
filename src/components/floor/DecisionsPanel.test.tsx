import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeSeedSnapshot } from "../../api/mockData";
import type { Handoff, OfficeSnapshot, Question } from "../../types";
import { DecisionsPanel } from "./DecisionsPanel";

const NOW = new Date("2026-09-28T16:00:00Z");
const base = makeSeedSnapshot();
const asker = base.sessions[1]!;

const row = (over: Partial<Question>): Question => ({
  id: "row-1",
  projectId: asker.projectId,
  sessionId: asker.id,
  prompt: "PROMPT-ONE",
  options: [],
  allowOther: false,
  answerVia: "terminal",
  priority: 0,
  askedAt: "2026-09-28T15:50:00Z",
  context: "",
  sourceFile: null,
  handoff: null,
  askedBy: null,
  ...over,
});

function show(q: Question, snapshot: OfficeSnapshot = base) {
  const onOpenSession = vi.fn();
  const onOpenRequest = vi.fn();
  render(<DecisionsPanel snapshot={snapshot} questions={[q]} decisions={[]} now={NOW} onAnswer={() => Promise.resolve()} onOpenSession={onOpenSession} onOpenRequest={onOpenRequest} />);
  return { onOpenSession, onOpenRequest };
}

describe("Decisions rows answered in a chat desk", () => {
  const chat = row({ answerVia: "chat", handoff: { kind: "chat", approvalId: "7" } });

  it("opens the request itself and no detail", async () => {
    const { onOpenRequest, onOpenSession } = show(chat);
    expect(screen.getByTestId("question-row")).toHaveTextContent("answered in its conversation");
    await userEvent.setup().click(screen.getByTestId("question-row"));
    expect(onOpenRequest).toHaveBeenCalledWith(asker, "7");
    expect(onOpenSession).not.toHaveBeenCalled();
    expect(screen.queryByTestId("question-detail")).toBeNull();
  });

  it("opens a detail that says its desk is gone, with nothing to press, when the desk left", async () => {
    show(chat, { ...base, sessions: base.sessions.filter((s) => s.id !== asker.id) });
    await userEvent.setup().click(screen.getByTestId("question-row"));
    expect(screen.getByText("Its desk is no longer here.")).toBeInTheDocument();
    expect(screen.queryByTestId("question-raise")).toBeNull();
  });
});

describe("Decisions rows raised by a helper or answered in the conversation", () => {
  it("names the helper in the list and in the detail", async () => {
    show(row({ askedBy: "Ada" }));
    expect(screen.getByTestId("question-row")).toHaveTextContent("by its helper Ada");
    await userEvent.setup().click(screen.getByTestId("question-row"));
    expect(screen.getByTestId("question-detail")).toHaveTextContent("by its helper Ada");
  });

  it("opens a conversation row's detail with the words and a button that opens the desk", async () => {
    const user = userEvent.setup();
    const { onOpenSession } = show(row({ answerVia: "chat", handoff: { kind: "conversation" } }));
    await user.click(screen.getByTestId("question-row"));
    expect(screen.getByText("Reply in its conversation.")).toBeInTheDocument();
    await user.click(screen.getByTestId("question-raise"));
    expect(onOpenSession).toHaveBeenCalledWith(asker);
    expect(screen.queryByTestId("question-other-submit")).toBeNull();
  });
});

describe("Decisions rows answered in a terminal", () => {
  const cases: readonly (readonly [string, Handoff | null, string, string | null])[] = [
    ["docked", { kind: "docked" }, "This agent runs in one of the app's terminals.", "Open its terminal"],
    ["a window with a tab", { kind: "window", app: "TERM-APP", tab: true }, "The app can bring its tab to the front.", "Bring its TERM-APP tab to the front"],
    ["a window without one", { kind: "window", app: "TERM-APP", tab: false }, "but cannot pick the tab.", "Bring TERM-APP to the front"],
    ["elsewhere", { kind: "elsewhere" }, "found no terminal window to bring forward", null],
    ["read-only", { kind: "read-only" }, "The building only reads this session.", null],
  ];

  it.each(cases)("says how to get there for %s", async (_name, handoff, hint, action) => {
    show(row({ handoff, context: "CONTEXT-LINE" }));
    await userEvent.setup().click(screen.getByTestId("question-row"));
    expect(screen.getByText(new RegExp(hint.replace(/[.]/g, "\\.")))).toBeInTheDocument();
    expect(screen.getByText("CONTEXT-LINE")).toBeInTheDocument();
    if (action === null) expect(screen.queryByTestId("question-raise")).toBeNull();
    else expect(screen.getByTestId("question-raise")).toHaveTextContent(action);
  });

  it("leaves the decision open when its terminal is opened", async () => {
    const user = userEvent.setup();
    const { onOpenSession } = show(row({ handoff: { kind: "docked" } }));
    await user.click(screen.getByTestId("question-row"));
    await user.click(screen.getByTestId("question-raise"));
    expect(onOpenSession).toHaveBeenCalledWith(asker);
    expect(screen.getByTestId("question-detail")).toBeInTheDocument();
    expect(screen.getByTestId("question-raise")).toBeInTheDocument();
  });
});
