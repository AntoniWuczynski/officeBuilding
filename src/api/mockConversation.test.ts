import { describe, expect, it, vi } from "vitest";
import { MockConversations } from "./mockConversation";

function desk(): { readonly conversations: MockConversations; readonly changed: ReturnType<typeof vi.fn> } {
  const changed = vi.fn();
  const conversations = new MockConversations(changed);
  conversations.start("s", "");
  return { conversations, changed };
}
const projectOf = (id: string): string | undefined => (id === "s" ? "p" : undefined);
const open = (c: MockConversations): { id: string; background: boolean; failed: string | null }[] => {
  const b = c.backlog("s");
  return b instanceof Error ? [] : [...b.approvals];
};

describe("MockConversations requests", () => {
  it("holds both requests of a prompt that asks and runs, the question first and in the background", () => {
    const { conversations } = desk();
    conversations.prompt("s", "ask and run");
    expect(open(conversations).map((a) => a.background)).toEqual([true, false]);
    expect(conversations.questions(projectOf).map((q) => [q.answerVia, q.priority, q.handoff?.kind])).toEqual([
      ["chat", 1, "chat"],
      ["chat", 0, "chat"],
    ]);
  });

  it("answers only the request it names and replies when none is left", () => {
    const { conversations } = desk();
    conversations.prompt("s", "ask and run");
    const [first, second] = open(conversations);
    expect(conversations.answer("s", second?.id ?? "", "deny", {})).toBeNull();
    expect(open(conversations).map((a) => a.id)).toEqual([first?.id]);
    expect(conversations.answer("s", second?.id ?? "", "deny", {})).toBeInstanceOf(Error);
    expect(conversations.answer("s", first?.id ?? "", "send", { db: "SQLite" })).toBeNull();
    expect(open(conversations)).toEqual([]);
    expect(conversations.questions(projectOf)).toEqual([]);
  });

  it("keeps the request and says why when the next answer is made to fail", () => {
    const { conversations } = desk();
    conversations.prompt("s", "run it");
    const id = open(conversations)[0]?.id ?? "";
    conversations.failNextAnswer("WRITE-FAILED");
    expect(conversations.answer("s", id, "allow", {})?.message).toBe("WRITE-FAILED");
    expect(open(conversations)[0]?.failed).toBe("WRITE-FAILED");
    expect(conversations.questions(projectOf)[0]?.context).toBe("WRITE-FAILED");
    expect(conversations.answer("s", id, "allow", {})).toBeNull();
  });

  it("publishes no row for a desk whose floor is unknown", () => {
    const { conversations } = desk();
    conversations.prompt("s", "run it");
    expect(conversations.questions(() => undefined)).toEqual([]);
  });
});
