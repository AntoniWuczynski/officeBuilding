import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ConversationEntry } from "../../types";
import { ToolCard } from "./ToolCard";

type Tool = Extract<ConversationEntry, { kind: "tool" }>;
const tool = (over: Partial<Tool>): Tool => ({ kind: "tool", id: "t", title: "pnpm test", status: "done", detail: "ok", diffs: [], ...over });

describe("ToolCard", () => {
  it("is closed until opened, then shows the output as text and one diff per file", async () => {
    const user = userEvent.setup();
    const entry = tool({
      detail: "<b>bold</b> output",
      diffs: [
        { path: "a.txt", text: "-x\n+y" },
        { path: "b.txt", text: "+z" },
      ],
    });
    const { container } = render(<ToolCard entry={entry} busy={false} />);
    const details = container.querySelector("details");
    expect(details?.open).toBe(false);
    await user.click(screen.getByText("pnpm test"));
    expect(details?.open).toBe(true);
    expect(container.querySelector(".chat-output")?.textContent).toBe("<b>bold</b> output");
    expect(container.querySelector("b")).toBeNull();
    expect(container.querySelectorAll(".chat-diff")).toHaveLength(2);
  });

  it("reads Stopped for an unfinished tool on a desk that is not busy", () => {
    const { rerender } = render(<ToolCard entry={tool({ status: "running" })} busy={false} />);
    expect(screen.getByText("Stopped")).toBeInTheDocument();
    rerender(<ToolCard entry={tool({ status: "running" })} busy />);
    expect(screen.getByText("Running")).toBeInTheDocument();
  });

  it("reads Waiting, not Stopped, for a tool waiting on an approval", () => {
    render(<ToolCard entry={tool({ status: "pending" })} busy />);
    expect(screen.getByText("Waiting")).toBeInTheDocument();
  });

  it("reads Failed for a failed tool", () => {
    render(<ToolCard entry={tool({ status: "failed" })} busy={false} />);
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });
});
