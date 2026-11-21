import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { OwnNeedsYou } from "./needsYouHarness";
import { makeSeedSnapshot } from "../../api/mockData";
import type { OfficeSnapshot, Question } from "../../types";

function show(snapshot: OfficeSnapshot, onAnswer = vi.fn(() => Promise.resolve()), onOpen = vi.fn()) {
  render(<OwnNeedsYou snapshot={snapshot} onOpen={onOpen} onAnswer={onAnswer} />);
  return { onAnswer, onOpen };
}
const rows = (): string[] => screen.getAllByTestId("needs-you-row").map((r) => r.getAttribute("data-question") ?? "");
const ask = (id: string, over: Partial<Question>): Question => ({ ...makeSeedSnapshot().questions[1], ...over, id }) as Question;

describe("the Needs-you list and a chat desk's request", () => {
  const chat = (id: string, approvalId: string): Question => ask(id, { answerVia: "chat", handoff: { kind: "chat", approvalId }, sourceFile: null });

  it("enables Answer on a chat row and opens the request with it", async () => {
    const { onOpen } = show({ ...makeSeedSnapshot(), questions: [chat("chat-row", "7")] });
    const user = userEvent.setup();
    await user.click(screen.getByTestId("needs-you"));
    const answer = within(screen.getByTestId("needs-you-row")).getByRole("button", { name: "Answer" });
    expect(answer).toBeEnabled();
    expect(answer).toHaveAttribute("title", "Answered in its conversation");
    await user.click(answer);
    expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: "chat-row", handoff: { kind: "chat", approvalId: "7" } }));
  });

  it("disables Answer on a terminal row with the handoff's hint, and lists each request once", async () => {
    const terminal = ask("term-row", { answerVia: "terminal", handoff: { kind: "docked" } });
    show({ ...makeSeedSnapshot(), questions: [terminal, chat("chat-row", "7")] });
    await userEvent.setup().click(screen.getByTestId("needs-you"));
    expect(rows()).toEqual(["chat-row", "term-row"].sort((a, b) => rows().indexOf(a) - rows().indexOf(b)));
    expect(rows()).toHaveLength(2);
    const row = screen.getAllByTestId("needs-you-row").find((r) => r.getAttribute("data-question") === "term-row") as HTMLElement;
    const answer = within(row).getByRole("button", { name: "Answer" });
    expect(answer).toBeDisabled();
    expect(answer).toHaveAttribute("title", expect.stringContaining("one of the app's terminals"));
  });
});

describe("the Needs-you list", () => {
  it("counts what waits and lists nothing until opened", () => {
    show(makeSeedSnapshot());
    expect(screen.getByRole("button", { name: "Needs you 3" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("needs-you-row")).not.toBeInTheDocument();
  });

  it("puts live session asks before decisions-file calls, newest first within each", async () => {
    const base = makeSeedSnapshot();
    const snapshot = {
      ...base,
      questions: [
        ask("file-new", { sessionId: "", answerVia: "file", askedAt: "2026-10-01T00:00:00Z", sourceFile: "D.md" }),
        ask("live-old", { askedAt: "2026-09-01T00:00:00Z" }),
        ask("file-old", { sessionId: "", answerVia: "file", askedAt: "2026-08-01T00:00:00Z", sourceFile: "D.md" }),
        ask("live-new", { askedAt: "2026-09-20T00:00:00Z", answerVia: "terminal" }),
      ],
    };
    show(snapshot);
    await userEvent.setup().click(screen.getByTestId("needs-you"));
    expect(rows()).toEqual(["live-new", "live-old", "file-new", "file-old"]);
  });

  it("opens what asks: the call's own row says where it came from", async () => {
    const { onOpen } = show(makeSeedSnapshot());
    const user = userEvent.setup();
    await user.click(screen.getByTestId("needs-you"));
    const file = screen.getAllByTestId("needs-you-row")[2] as HTMLElement;
    expect(file).toHaveTextContent("officeBuilding: From FOUNDER_DECISIONS.md");
    await user.click(within(file).getByRole("button", { name: "Open" }));
    expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: "q-3" }));
    // Choosing a row puts the list away.
    expect(screen.queryByTestId("needs-you-row")).not.toBeInTheDocument();
  });

  it("answers a row with the question's id, and shows why when the answer is refused", async () => {
    const onAnswer = vi.fn(() => Promise.reject(new Error("that call is kept by the building")));
    show(makeSeedSnapshot(), onAnswer);
    const user = userEvent.setup();
    await user.click(screen.getByTestId("needs-you"));
    const first = screen.getAllByTestId("needs-you-row")[0] as HTMLElement;
    await user.click(within(first).getByRole("button", { name: "Answer" }));
    await user.type(within(first).getByRole("textbox", { name: "Your own answer" }), "Do it{Enter}");
    expect(onAnswer).toHaveBeenCalledWith("q-1", { kind: "other", text: "Do it" });
    expect(await within(first).findByRole("alert")).toHaveTextContent("Answer not sent: that call is kept by the building");
  });

  it("drops a half-typed answer on Escape, and only the next Escape puts the list away", async () => {
    show(makeSeedSnapshot());
    const user = userEvent.setup();
    await user.click(screen.getByTestId("needs-you"));
    const first = screen.getAllByTestId("needs-you-row")[0] as HTMLElement;
    await user.click(within(first).getByRole("button", { name: "Answer" }));
    const field = within(first).getByRole("textbox", { name: "Your own answer" });
    await user.type(field, "half{Escape}");
    expect(field).toHaveValue("");
    expect(screen.getAllByTestId("needs-you-row")).not.toHaveLength(0);
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("needs-you-row")).not.toBeInTheDocument();
  });

  it("cannot answer a hand-started agent's question here: its terminal is the place", async () => {
    show(makeSeedSnapshot());
    await userEvent.setup().click(screen.getByTestId("needs-you"));
    const tile = screen.getAllByTestId("needs-you-row")[1] as HTMLElement;
    expect(within(tile).getByRole("button", { name: "Answer" })).toBeDisabled();
    expect(within(tile).getByRole("button", { name: "Open" })).toBeEnabled();
  });

  it("says so when nothing waits", async () => {
    show({ ...makeSeedSnapshot(), questions: [] });
    await userEvent.setup().click(screen.getByRole("button", { name: "Needs you" }));
    expect(screen.getByText("Nothing is waiting on you.")).toBeInTheDocument();
  });
});
