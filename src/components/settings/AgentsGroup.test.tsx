import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AgentsGroup } from "./AgentsGroup";

describe("AgentsGroup", () => {
  it("checks the saved view and shows the hint", () => {
    render(<AgentsGroup value={{ view: "chat" }} onChange={() => {}} />);
    expect(screen.getByRole("radio", { name: "Chat" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Terminal" })).not.toBeChecked();
    expect(screen.getByText(/New desks open in this view/)).toBeInTheDocument();
  });

  it("reports a chosen view", async () => {
    const onChange = vi.fn();
    render(<AgentsGroup value={{ view: "terminal" }} onChange={onChange} />);
    await userEvent.setup().click(screen.getByRole("radio", { name: "Chat" }));
    expect(onChange).toHaveBeenCalledWith({ view: "chat" });
  });
});
