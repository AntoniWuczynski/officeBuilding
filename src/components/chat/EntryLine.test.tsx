import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EntryLine } from "./EntryLine";

const inList = (ui: React.ReactElement): ReturnType<typeof render> => render(<ol>{ui}</ol>);

describe("EntryLine", () => {
  it("draws an agent message as Markdown", () => {
    const { container } = inList(<EntryLine entry={{ kind: "agent", id: "a", text: "- one\n- two" }} busy={false} />);
    expect(container.querySelectorAll("li li")).toHaveLength(2);
  });

  it("draws a user message as plain text", () => {
    const { container } = inList(<EntryLine entry={{ kind: "user", id: "u", text: "**stars**" }} busy={false} />);
    expect(screen.getByText("**stars**")).toBeInTheDocument();
    expect(container.querySelector("strong")).toBeNull();
  });

  it("keeps a thought closed until it is clicked", async () => {
    const user = userEvent.setup();
    const { container } = inList(<EntryLine entry={{ kind: "thought", id: "t", text: "pondering" }} busy={false} />);
    const details = container.querySelector("details");
    expect(details?.open).toBe(false);
    await user.click(screen.getByText("Thinking"));
    expect(details?.open).toBe(true);
  });

  it("cuts a very long agent message behind Show the whole message", async () => {
    const user = userEvent.setup();
    const { container } = inList(<EntryLine entry={{ kind: "agent", id: "a", text: "x".repeat(9000) }} busy={false} />);
    expect(container.textContent).toHaveLength(8000 + "Show the whole message".length);
    await user.click(screen.getByRole("button", { name: "Show the whole message" }));
    expect(container.textContent).toHaveLength(9000);
  });
});
