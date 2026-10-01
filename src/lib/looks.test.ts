import { describe, expect, it } from "vitest";
import { hashString, lookFor } from "./looks";
import { relativeTime } from "./time";

describe("lookFor", () => {
  it("is stable for an id", () => {
    expect(lookFor("s-1")).toEqual(lookFor("s-1"));
  });
  it("varies across ids", () => {
    const shirts = new Set(["s-1", "s-2", "s-3", "s-4", "s-5", "s-6", "s-7", "s-8"].map((id) => lookFor(id).shirt));
    expect(shirts.size).toBeGreaterThan(2);
  });
  it("hashString is FNV-1a", () => {
    expect(hashString("")).toBe(0x811c9dc5);
    expect(hashString("a")).toBe(0xe40c292c);
  });
});

describe("relativeTime", () => {
  const now = new Date("2026-09-29T12:00:00Z");
  it("reads in plain units", () => {
    expect(relativeTime("2026-09-29T11:59:30Z", now)).toBe("under a minute ago");
    expect(relativeTime("2026-09-29T11:56:00Z", now)).toBe("4 min ago");
    expect(relativeTime("2026-09-29T09:00:00Z", now)).toBe("3 h ago");
    expect(relativeTime("2026-09-28T11:00:00Z", now)).toBe("1 day ago");
    expect(relativeTime("2026-09-26T11:00:00Z", now)).toBe("3 days ago");
  });
  it("never goes negative and admits bad input", () => {
    expect(relativeTime("2026-09-30T00:00:00Z", now)).toBe("under a minute ago");
    expect(relativeTime("not a date", now)).toBe("at an unknown time");
  });
});
