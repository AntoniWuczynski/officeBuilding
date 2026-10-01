import { describe, expect, it } from "vitest";
import { shellPaths } from "./shell";

describe("shellPaths", () => {
  it("escapes dropped paths the way Terminal.app types them", () => {
    expect(shellPaths(["/Users/me/Desktop/Screenshot 2026-10-01 at 15.53.18.png"])).toBe(
      "/Users/me/Desktop/Screenshot\\ 2026-10-01\\ at\\ 15.53.18.png",
    );
    expect(shellPaths(["/tmp/a (1)'s & b.png", "/tmp/plain.png"])).toBe("/tmp/a\\ \\(1\\)\\'s\\ \\&\\ b.png /tmp/plain.png");
  });
  it("leaves non-ASCII characters alone", () => {
    expect(shellPaths(["/tmp/Zrzut ekranu 3.04 PM é.png"])).toBe("/tmp/Zrzut\\ ekranu\\ 3.04 PM\\ é.png");
  });
});
