import { describe, expect, it } from "vitest";
import { conversationBacklogSchema, snapshotSchema } from "./wire";
import { makeSeedSnapshot } from "./mockData";
import type { Approval, ConversationBacklog, Helper, Question } from "../types";

const helper: Helper = { id: "Ada", state: "running", worktree: false, branch: null, model: "MODEL-ONE", effort: "high" };

describe("the wire schemas for a helper and who asked", () => {
  it("parses a snapshot with a helper's model and effort, a question's askedBy and a conversation handoff", () => {
    const seed = makeSeedSnapshot();
    const [first, ...rest] = seed.sessions;
    const question: Question = { ...(seed.questions[0] as Question), askedBy: "Ada", handoff: { kind: "conversation" }, answerVia: "chat" };
    const snapshot = { ...seed, sessions: [{ ...first, helpers: [helper] }, ...rest], questions: [question] };
    const parsed = snapshotSchema.parse(snapshot);
    expect(parsed.sessions[0]?.helpers[0]).toEqual(helper);
    expect(parsed.questions[0]).toMatchObject({ askedBy: "Ada", handoff: { kind: "conversation" } });
  });

  it("refuses a helper without a model", () => {
    const seed = makeSeedSnapshot();
    const [first, ...rest] = seed.sessions;
    const { model: _model, ...bare } = helper;
    expect(snapshotSchema.safeParse({ ...seed, sessions: [{ ...first, helpers: [bare] }, ...rest] }).success).toBe(false);
  });

  it("parses a backlog whose approval says which helper asked, and refuses one that does not say", () => {
    const approval: Approval = { id: "a1", title: "T", detail: "", fields: [], choices: [], background: false, failed: null, askedBy: "Ada" };
    const backlog: ConversationBacklog = {
      transcript: [],
      state: "running",
      approvals: [approval],
      ended: null,
      capabilities: { compose: true, stop: true, end: true, approvals: "inline", terminal: false },
      earlier: false,
    };
    expect(conversationBacklogSchema.parse(backlog).approvals[0]?.askedBy).toBe("Ada");
    const { askedBy: _askedBy, ...bare } = approval;
    expect(conversationBacklogSchema.safeParse({ ...backlog, approvals: [bare] }).success).toBe(false);
  });
});
