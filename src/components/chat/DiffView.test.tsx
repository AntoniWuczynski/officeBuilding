import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { DiffView } from "./DiffView";

describe("DiffView", () => {
  it("colours added, removed and hunk lines and shows the path", () => {
    const { container } = render(
      <DiffView
        diff={{
          path: "src/a.ts",
          text: "@@ -1 +1 @@\n-old <b>x</b>\n+new\n same",
        }}
      />,
    );
    expect(screen.getByText("src/a.ts")).toBeInTheDocument();
    expect(container.querySelector(".is-hunk")?.textContent).toContain("@@");
    expect(container.querySelector(".is-del")?.textContent).toContain("old <b>x</b>");
    expect(container.querySelector(".is-add")?.textContent).toContain("new");
    expect(container.querySelector("b")).toBeNull();
    expect(container.querySelectorAll("span:not([class])")).toHaveLength(1);
  });

  it("collapses a long diff behind Show all", () => {
    const text = Array.from({ length: 60 }, (_, i) => `+l${i}`).join("\n");
    const { container } = render(<DiffView diff={{ path: "b.ts", text }} />);
    expect(container.querySelectorAll(".is-add")).toHaveLength(40);
    expect(screen.getByRole("button", { name: "Show all 60 lines" })).toBeInTheDocument();
  });
});
