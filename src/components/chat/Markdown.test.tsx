import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { Markdown } from "./Markdown";

function draw(text: string): HTMLElement {
  return render(<Markdown text={text} />).container;
}

describe("Markdown", () => {
  it("draws emphasis and lists", () => {
    const root = draw("**bold** and a list\n\n- one\n- two");
    expect(root.querySelector("strong")?.textContent).toBe("bold");
    expect(root.querySelectorAll("li")).toHaveLength(2);
  });

  it("draws a fenced code block as pre > code", () => {
    const root = draw("```ts\nconst a = 1;\n```");
    expect(root.querySelector("pre > code")?.textContent).toContain("const a = 1;");
  });

  it("draws a table and a task list box", () => {
    const root = draw("| a | b |\n| - | - |\n| 1 | 2 |\n\n- [x] done");
    expect(root.querySelector("table")).not.toBeNull();
    const box = root.querySelector<HTMLInputElement>("input[type=checkbox]");
    expect(box?.checked).toBe(true);
    expect(box?.disabled).toBe(true);
  });

  it("drops raw HTML", () => {
    const root = draw("<script>alert(1)</script><b>x</b><img src=x onerror=alert(1)>");
    expect(root.querySelector("script, b, img")).toBeNull();
    expect(root.querySelector("[onerror]")).toBeNull();
  });

  it("draws a link as text with its address and never as an anchor", () => {
    const root = draw("[site](https://example.com/a)");
    expect(root.textContent).toContain("site (https://example.com/a)");
    expect(root.querySelector("a")).toBeNull();
  });

  it("drops a javascript address", () => {
    const root = draw("[x](javascript:alert(1))");
    expect(root.textContent).toContain("x");
    expect(root.querySelector("a")).toBeNull();
    expect(root.innerHTML).not.toContain("javascript:");
  });

  it("draws an image as words and never loads it", () => {
    const root = draw("![a chart](https://example.com/p.png)");
    expect(root.textContent).toContain("image: a chart (https://example.com/p.png)");
    expect(root.querySelector("img")).toBeNull();
  });

  it("shows an autolink's address once", () => {
    const root = draw("<https://example.com>");
    expect(root.textContent?.match(/https:\/\/example\.com/g)).toHaveLength(1);
  });
});
