import { describe, expect, it } from "vitest";

const sources = import.meta.glob("/src/**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true });

describe("source safety", () => {
  it("never injects raw HTML", () => {
    const banned = ["dangerouslySetInnerHTML", "rehype-raw"];
    const hits = Object.entries(sources)
      .filter(([path]) => !path.endsWith("/chat/safety.test.ts"))
      .filter(([, text]) => typeof text === "string" && banned.some((word) => text.includes(word)))
      .map(([path]) => path);
    expect(hits).toEqual([]);
    expect(Object.keys(sources).length).toBeGreaterThan(50);
  });
});
