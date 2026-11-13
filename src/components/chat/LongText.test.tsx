import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { LONG_LINES, LongText } from "./LongText";

const lines = (n: number): string => Array.from({ length: n }, (_, i) => `line ${i + 1}`).join("\n");

describe("LongText", () => {
  it("leaves short text alone", () => {
    render(<LongText text={lines(3)} />);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("collapses, expands and collapses again", async () => {
    const user = userEvent.setup();
    const { container } = render(<LongText text={lines(212)} className="x" />);
    const pre = container.querySelector("pre");
    expect(pre?.className).toBe("x");
    expect(pre?.textContent?.split("\n")).toHaveLength(LONG_LINES);
    await user.click(screen.getByRole("button", { name: "Show all 212 lines" }));
    expect(pre?.textContent?.split("\n")).toHaveLength(212);
    await user.click(screen.getByRole("button", { name: "Show less" }));
    expect(pre?.textContent?.split("\n")).toHaveLength(LONG_LINES);
  });

  it("collapses one very long line by characters", () => {
    const { container } = render(<LongText text={"x".repeat(9000)} />);
    expect(container.querySelector("pre")?.textContent).toHaveLength(4000);
    expect(screen.getByRole("button", { name: "Show all 1 lines" })).toBeInTheDocument();
  });
});
