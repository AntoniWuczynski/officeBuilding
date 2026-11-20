import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TakeOverStrip } from "./TakeOverStrip";

describe("TakeOverStrip", () => {
  it("renders nothing with no lines and no callback", () => {
    const { container } = render(<TakeOverStrip lines={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the lines with a lamp, and no button without a callback", () => {
    const { container } = render(<TakeOverStrip lines={["One.", "Two."]} />);
    expect(screen.getByTestId("take-over-strip")).toHaveTextContent("One.Two.");
    expect(container.querySelector(".route-lamp")).toHaveAttribute("data-tone", "ok");
    expect(screen.queryByTestId("take-over-button")).not.toBeInTheDocument();
  });

  it("shows the button only with a callback, and calls it", async () => {
    const onTakeOver = vi.fn();
    const { container } = render(<TakeOverStrip lines={[]} onTakeOver={onTakeOver} />);
    expect(container.querySelector(".route-lamp")).toBeNull();
    await userEvent.click(screen.getByTestId("take-over-button"));
    expect(screen.getByTestId("take-over-button")).toHaveTextContent("Take over with a new session");
    expect(onTakeOver).toHaveBeenCalledOnce();
  });
});
