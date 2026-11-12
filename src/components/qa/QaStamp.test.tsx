import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import type { Verdict } from "../../types";
import { QaStamp } from "./QaStamp";

describe("QaStamp", () => {
  it.each<[Verdict, string, string]>([
    ["pass", "QA passed", "QA passed"],
    ["concerns", "QA notes", "QA found problems"],
  ])("letters a %s verdict", (verdict, label, spoken) => {
    render(<QaStamp verdict={verdict} />);
    const stamp = screen.getByTestId("qa-stamp");
    expect(stamp).toHaveTextContent(label);
    expect(stamp).toHaveAttribute("title", spoken);
    expect(stamp).toHaveClass("qa-stamp", `is-${verdict}`);
  });
});
