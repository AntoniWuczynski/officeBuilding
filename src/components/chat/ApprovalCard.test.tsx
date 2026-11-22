import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Approval } from "../../types";
import { ApprovalCard } from "./ApprovalCard";

const base: Approval = {
  id: "a1",
  title: "TITLE-ONE",
  detail: "",
  fields: [],
  choices: [
    { id: "ok", label: "Allow once", allow: true },
    { id: "no", label: "Decline", allow: false },
  ],
  background: false,
  failed: null, askedBy: null,
};

function show(approval: Approval, onAnswer = vi.fn(() => Promise.resolve()), extra: { focused?: boolean } = {}): ReturnType<typeof vi.fn> {
  render(<ApprovalCard approval={approval} mode="inline" onAnswer={onAnswer} {...extra} />);
  return onAnswer;
}
const button = (name: string): HTMLElement => screen.getByRole("button", { name });

describe("ApprovalCard", () => {
  it("says which helper asked, and nothing more when the session itself did", () => {
    const { unmount } = render(<ApprovalCard approval={{ ...base, askedBy: "Ada" }} mode="inline" onAnswer={vi.fn(() => Promise.resolve())} />);
    expect(screen.getByText("Asked by its helper Ada.")).toBeInTheDocument();
    expect(screen.getByRole("group")).toHaveAccessibleName(/asked by its helper Ada/);
    unmount();
    show(base);
    expect(screen.queryByText(/Asked by its helper/)).toBeNull();
    expect(screen.getByRole("group")).toHaveAccessibleName("The agent asks: TITLE-ONE");
  });

  it("closes every choice at a click and leaves them closed once the answer was sent", async () => {
    const answer = show(base);
    await userEvent.setup().click(button("Allow once"));
    expect(answer).toHaveBeenCalledWith("ok", {});
    expect(button("Allow once")).toBeDisabled();
    expect(button("Decline")).toBeDisabled();
  });

  it("opens the choices again when the answer is refused", async () => {
    show(base, vi.fn(() => Promise.reject(new Error("REFUSED"))));
    await userEvent.setup().click(button("Allow once"));
    await waitFor(() => expect(button("Decline")).toBeEnabled());
    expect(button("Allow once")).toBeEnabled();
  });

  it("shows why the last answer failed and keeps the choices usable", () => {
    show({ ...base, failed: "WRITE-FAILED" });
    expect(screen.getByRole("alert")).toHaveTextContent("Not sent: WRITE-FAILED. You can send it again.");
    expect(button("Allow once")).toBeEnabled();
  });

  it("says a background request does not stop the agent", () => {
    show({ ...base, background: true });
    expect(screen.getByText("The agent keeps working while this is open.")).toBeInTheDocument();
  });

  it("shows each field's own detail under its label and hides a secret pick's own answer", async () => {
    const field = (id: string, secret: boolean) => ({
      id,
      label: `LABEL-${id}`,
      detail: `DETAIL-${id}`,
      kind: { kind: "pick" as const, options: ["OPTION-A"], other: true, secret },
      required: false,
    });
    show({ ...base, fields: [field("one", false), field("two", true)] });
    expect(screen.getByText("DETAIL-one")).toHaveClass("conversation-field-detail");
    expect(screen.getByText("DETAIL-two")).toBeInTheDocument();
    const user = userEvent.setup();
    await user.selectOptions(screen.getByLabelText(/LABEL-two/), "Something else…");
    expect(screen.getByLabelText("LABEL-two: your own answer")).toHaveAttribute("type", "password");
  });

  it("moves focus into a focused card", () => {
    show({ ...base, fields: [{ id: "f", label: "LABEL", detail: "", kind: { kind: "text" }, required: false }] }, undefined, { focused: true });
    expect(screen.getByTestId("conversation-approval")).toHaveAttribute("data-focused", "true");
    expect(screen.getByLabelText("LABEL")).toHaveFocus();
  });

  it("focuses the card, never a choice button, when it has no field", () => {
    show(base, undefined, { focused: true });
    expect(screen.getByTestId("conversation-approval")).toHaveFocus();
  });

  it("has no button when it has no choices", () => {
    show({ ...base, choices: [], detail: "STATUS-DETAIL" });
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("STATUS-DETAIL")).toBeInTheDocument();
  });
});
