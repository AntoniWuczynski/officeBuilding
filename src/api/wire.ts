import { z } from "zod";
import type { OfficeSnapshot, Project, Session, ToolOptions } from "../types";
import type { HireFailure } from "./DiscoveryApi";

// Runtime schemas for everything the Rust backend sends. The `satisfies`
// clauses make the compiler prove each schema produces exactly our types, so
// the wire check and types.ts cannot drift apart.

const toolKind = z.enum(["claude-code", "codex", "opencode", "antigravity", "cursor", "unknown"]);
const sessionState = z.enum(["error", "waiting-human", "running", "thinking", "idle", "done"]);
const party = z.enum(["ai", "human"]);

export const sessionSchema = z.object({
  id: z.string(),
  tool: toolKind,
  projectId: z.string(),
  title: z.string(),
  state: sessionState,
  control: z.enum(["full", "raise-window", "read-only"]),
  model: z.string().nullable(),
  effort: z.string().nullable(),
  lastActivityAt: z.string(),
  pendingQuestionIds: z.array(z.string()),
  spend: z.object({ usd: z.number(), tokens: z.number(), unpricedTokens: z.number() }),
  helpers: z.array(z.object({ id: z.string(), state: sessionState })),
}) satisfies z.ZodType<Session>;

export const projectSchema = z.object({
  id: z.string(),
  name: z.string(),
  path: z.string(),
  sessionIds: z.array(z.string()),
}) satisfies z.ZodType<Project>;

export const snapshotSchema = z.object({
  projects: z.array(projectSchema),
  sessions: z.array(sessionSchema),
  ended: z.array(
    z.object({
      id: z.string(),
      tool: toolKind,
      projectId: z.string(),
      title: z.string(),
      endedAt: z.string(),
      model: z.string().nullable(),
      effort: z.string().nullable(),
    }),
  ),
  questions: z.array(
    z.object({
      id: z.string(),
      projectId: z.string(),
      sessionId: z.string(),
      prompt: z.string(),
      options: z.array(z.object({ id: z.string(), label: z.string() })),
      allowOther: z.boolean(),
      answerVia: z.enum(["app", "terminal", "file"]),
      priority: z.number(),
      askedAt: z.string(),
      context: z.string(),
      sourceFile: z.string().nullable(),
    }),
  ),
  decisions: z.array(
    z.object({ id: z.string(), projectId: z.string(), title: z.string(), detail: z.string(), decidedAt: z.string(), by: party }),
  ),
  todos: z.array(
    z.object({ id: z.string(), projectId: z.string(), text: z.string(), done: z.boolean(), assignee: party, sourceFile: z.string().nullable() }),
  ),
  messages: z.array(
    z.object({ id: z.string(), fromSessionId: z.string(), toSessionId: z.string(), text: z.string(), sentAt: z.string() }),
  ),
}) satisfies z.ZodType<OfficeSnapshot>;

const effortSchema = z.object({ id: z.string(), label: z.string(), description: z.string() });

export const hireOptionsSchema = z.array(
  z.object({
    tool: toolKind,
    models: z.array(z.object({ id: z.string(), label: z.string(), efforts: z.array(effortSchema), defaultEffort: z.string() })),
    defaultModel: z.string(),
  }),
) satisfies z.ZodType<ToolOptions[]>;

export const folderSchema = z.string().nullable();
// In-app terminals (src-tauri/src/pty.rs). `seq` counts output chunks, so a
// panel can drop live chunks its backlog already holds.

export const terminalBacklogSchema = z.object({
  terminalId: z.string(),
  seq: z.number(),
  data: z.string(),
  running: z.boolean(),
  exitCode: z.number().nullable(),
  cols: z.number(),
  rows: z.number(),
});

export const terminalOutputSchema = z.object({ terminalId: z.string(), seq: z.number(), data: z.string() });

export const terminalExitSchema = z.object({ terminalId: z.string(), exitCode: z.number().nullable() });

export const hireFailureSchema = z.object({
  title: z.string(),
  exitCode: z.number().nullable(),
  lastLines: z.array(z.string()),
}) satisfies z.ZodType<HireFailure>;

/** Parse a backend payload, turning a shape mismatch into a readable error. */
export function parseWire<T>(schema: z.ZodType<T>, what: string, payload: unknown): T {
  const result = schema.safeParse(payload);
  if (!result.success) {
    const first = result.error.issues[0];
    const where = first === undefined ? "" : ` at ${first.path.join(".") || "the top level"}: ${first.message}`;
    throw new Error(`the backend sent an unexpected ${what}${where}`);
  }
  return result.data;
}
