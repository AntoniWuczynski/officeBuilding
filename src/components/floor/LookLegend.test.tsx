import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { LookLegend } from "./LookLegend";

describe("the look legend", () => {
  it("is folded and names each tool's wood in words", () => {
    render(<LookLegend />);
    const legend = screen.getByTestId("look-legend");
    expect(legend).not.toHaveAttribute("open");
    for (const line of ["Claude Code: walnut", "Codex: oak", "Cursor: wenge", "opencode: pale maple", "Antigravity: cherry", "Unknown tool: pine"]) expect(legend).toHaveTextContent(line);
  });
  it("names both families of each tier", () => {
    render(<LookLegend />);
    for (const line of ["Haiku and Luna", "Sonnet and Terra", "Opus and Sol", "Fable and Astra"]) expect(screen.getByTestId("look-legend")).toHaveTextContent(line);
  });
  it("lists the six builds from lean to buff", () => {
    render(<LookLegend />);
    const builds = screen.getByTestId("look-legend").querySelector(".legend-builds");
    expect(within(builds as HTMLElement).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Minimal", "Low", "Medium", "High", "Extra high", "Max"]);
  });
  it("says what the tint and the manager are, and leaves every image decorative", () => {
    render(<LookLegend />);
    const legend = screen.getByTestId("look-legend");
    expect(legend).toHaveTextContent("A purple tint: Claude Code, hired here with ultracode on.");
    expect(legend).toHaveTextContent("The manager always looks the same.");
    const images = legend.querySelectorAll("img");
    expect(images).toHaveLength(10);
    for (const img of images) expect(img).toHaveAttribute("alt", "");
  });
});
