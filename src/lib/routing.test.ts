import { describe, expect, it } from "vitest";
import { makeRoutingSettings } from "../api/mockRouting";
import { nth } from "../test/nth";
import type { ConnectionKind, RoutingSettings } from "../types";
import {
  CONNECTIONS_MAX,
  KINDS,
  NAME_MAX,
  TIERS,
  addConnection,
  addContext,
  cleanName,
  formatEfforts,
  kindOffer,
  kindTraits,
  moveConnection,
  newId,
  parseEfforts,
  parseModels,
  patchConnection,
  removeConnection,
  removeContext,
  renameContext,
  setTarget,
  setTierModels,
} from "./routing";

function sample(): RoutingSettings {
  let s = addContext(makeRoutingSettings(), "ctx-1", "Work");
  s = addContext(s, "ctx-2", "Home");
  s = addConnection(s, "con-a", "ctx-1", "claude-subscription");
  s = addConnection(s, "con-b", "ctx-1", "anthropic-api-key");
  s = addConnection(s, "con-c", "ctx-1", "openrouter");
  return addConnection(s, "con-d", "ctx-2", "openai-api-key");
}
const chain = (s: RoutingSettings, id: string): readonly string[] => s.contexts.find((c) => c.id === id)?.chain ?? [];

describe("tables", () => {
  it("lists four tiers and five kinds", () => {
    expect(TIERS.map((t) => t.id)).toEqual(["fable", "opus", "sonnet", "haiku"]);
    expect(KINDS.map((k) => k.kind)).toHaveLength(5);
  });

  it("gives each kind its traits", () => {
    const rows: [ConnectionKind, boolean, boolean, boolean, boolean][] = [
      ["claude-subscription", false, false, false, true],
      ["anthropic-api-key", true, false, true, false],
      ["openrouter", true, false, true, false],
      ["openai-api-key", true, true, true, false],
      ["chatgpt-sign-in", false, true, false, false],
    ];
    for (const [kind, metered, experimental, holdsKey, passesThrough] of rows) {
      expect(kindTraits(kind), kind).toEqual({ metered, experimental, holdsKey, passesThrough });
    }
  });
});

describe("kindOffer", () => {
  it("refuses a second subscription, the sign-in and a full list", () => {
    const s = sample();
    expect(kindOffer(s, "claude-subscription")).toBe("The Claude login on this Mac can belong to one context only.");
    expect(kindOffer(s, "chatgpt-sign-in")).toBe("Sign in with ChatGPT is not built yet. OpenAI has to accept the building for ChatGPT plan use first.");
    expect(kindOffer(s, "openrouter")).toBeNull();
    expect(kindOffer(makeRoutingSettings(), "claude-subscription")).toBeNull();
    const full = { ...s, connections: Array.from({ length: CONNECTIONS_MAX }, (_, i) => ({ ...nth(s.connections, 1), id: `con-${i}` })) };
    expect(kindOffer(full, "openrouter")).toBe(`There can be at most ${CONNECTIONS_MAX} connections.`);
  });
});

describe("ids and names", () => {
  it("makes a prefixed id of 8 hex characters and retries a taken one", () => {
    const values = ["aaaaaaaa-1111", "bbbbbbbb-2222"];
    let i = 0;
    const random = (): string => values[i++] ?? "cccccccc";
    const taken = addConnection(addContext(makeRoutingSettings(), "ctx-aaaaaaaa", "Work"), "con-bbbbbbbb", "ctx-aaaaaaaa", "openrouter");
    expect(newId("ctx", taken, random)).toBe("ctx-bbbbbbbb");
    expect(newId("con", makeRoutingSettings())).toMatch(/^con-[0-9a-f]{8}$/);
    const again = ["bbbbbbbb-1", "cccccccc-2"];
    let j = 0;
    expect(newId("con", taken, () => again[j++] ?? "dddddddd")).toBe("con-cccccccc");
  });

  it("trims a name and refuses an empty or long one", () => {
    expect(cleanName("  Work ")).toBe("Work");
    expect(cleanName("   ")).toBeNull();
    expect(cleanName("x".repeat(NAME_MAX))).toBe("x".repeat(NAME_MAX));
    expect(cleanName("x".repeat(NAME_MAX + 1))).toBeNull();
  });
});

describe("editing", () => {
  it("adds a context and renames it", () => {
    const s = renameContext(addContext(makeRoutingSettings(), "ctx-1", "Work"), "ctx-1", "Job");
    expect(s.contexts).toEqual([{ id: "ctx-1", name: "Job", chain: [] }]);
  });

  it("adds a connection to the end of the order with no opt-in or targets", () => {
    const s = sample();
    expect(chain(s, "ctx-1")).toEqual(["con-a", "con-b", "con-c"]);
    expect(s.connections[1]).toEqual({
      id: "con-b",
      name: "Anthropic API key",
      kind: "anthropic-api-key",
      context: "ctx-1",
      paid: false,
      experimental: false,
      targets: { fable: null, opus: null, sonnet: null, haiku: null },
    });
  });

  it("removes a context with its connections", () => {
    const s = removeContext(sample(), "ctx-1");
    expect(s.contexts.map((c) => c.id)).toEqual(["ctx-2"]);
    expect(s.connections.map((c) => c.id)).toEqual(["con-d"]);
  });

  it("removes a connection from the list and every order, and leaves an unknown id alone", () => {
    const s = removeConnection(sample(), "con-b");
    expect(s.connections.map((c) => c.id)).toEqual(["con-a", "con-c", "con-d"]);
    expect(chain(s, "ctx-1")).toEqual(["con-a", "con-c"]);
    expect(removeConnection(sample(), "con-zz")).toEqual(sample());
  });

  it("moves a connection, stops at both ends and copes with one outside the order", () => {
    const s = sample();
    expect(chain(moveConnection(s, "con-b", -1), "ctx-1")).toEqual(["con-b", "con-a", "con-c"]);
    expect(chain(moveConnection(s, "con-b", 1), "ctx-1")).toEqual(["con-a", "con-c", "con-b"]);
    expect(moveConnection(s, "con-a", -1)).toEqual(s);
    expect(moveConnection(s, "con-c", 1)).toEqual(s);
    const stray = { ...s, contexts: s.contexts.map((c) => (c.id === "ctx-1" ? { ...c, chain: ["con-a"] } : c)) };
    expect(moveConnection(stray, "con-c", -1)).toEqual(stray);
  });

  it("patches a connection", () => {
    const s = patchConnection(sample(), "con-b", { name: "Key", paid: true });
    expect(s.connections[1]).toMatchObject({ name: "Key", paid: true, experimental: false });
    expect(s.connections[0]).toMatchObject({ name: "Claude subscription", paid: false });
  });

  it("sets a target and clears it with an empty model", () => {
    const set = setTarget(sample(), "con-b", "haiku", " my-model ", { xhigh: "high" });
    expect(set.connections[1]?.targets.haiku).toEqual({ model: "my-model", efforts: { xhigh: "high" } });
    expect(set.connections[1]?.targets.opus).toBeNull();
    const cleared = setTarget(set, "con-b", "haiku", "  ", {});
    expect(cleared.connections[1]?.targets.haiku).toBeNull();
  });

  it("sets the models of one tier", () => {
    expect(setTierModels(makeRoutingSettings(), "opus", ["a", "b"]).tiers).toEqual({ fable: [], opus: ["a", "b"], sonnet: [], haiku: [] });
  });
});

describe("text forms", () => {
  it("parses efforts", () => {
    const rows: [string, string, Record<string, string> | null][] = [
      ["good text", "xhigh=high, max = high", { xhigh: "high", max: "high" }],
      ["empty text", "  ", {}],
      ["stray comma", "a=b,", { a: "b" }],
      ["no equals sign", "xhigh", null],
      ["empty left", "=high", null],
      ["empty right", "xhigh=", null],
      ["two equals signs", "a=b=c", null],
    ];
    for (const [name, text, expected] of rows) expect(parseEfforts(text), name).toEqual(expected);
  });

  it("round-trips efforts", () => {
    const efforts = { xhigh: "high", max: "high" };
    expect(formatEfforts(efforts)).toBe("xhigh=high, max=high");
    expect(parseEfforts(formatEfforts(efforts))).toEqual(efforts);
    expect(formatEfforts({})).toBe("");
  });

  it("parses model names split by commas or white space, without repeats", () => {
    expect(parseModels(" a, b  c,,a\nd ")).toEqual(["a", "b", "c", "d"]);
    expect(parseModels("")).toEqual([]);
  });
});
