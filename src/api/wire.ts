import { z } from "zod";
import type { ConversationBacklog, Editor, LinearKeyStatus, LinearTeam, EditorList, ConversationUpdate, OfficeSnapshot, Project, QueuedNote, Session, Settings, SetupView, Shell, ToolOptions, ToolUsage } from "../types";
import type { HireFailure } from "./DiscoveryApi";
import { routingSettingsSchema } from "./routingWire";

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
  control: z.enum(["full", "raise-window", "read-only", "structured"]),
  model: z.string().nullable(),
  effort: z.string().nullable(),
  lastActivityAt: z.string(),
  pendingQuestionIds: z.array(z.string()),
  spend: z.object({ usd: z.number(), tokens: z.number(), unpricedTokens: z.number() }),
  helpers: z.array(z.object({ id: z.string(), state: sessionState, worktree: z.boolean(), branch: z.string().nullable() })),
  pets: z.array(z.object({ id: z.string(), kind: z.enum(["shell", "monitor", "browser", "process"]), label: z.string() })),
  worktree: z.boolean(),
  branch: z.string().nullable(),
}) satisfies z.ZodType<Session>;

export const projectSchema = z.object({
  id: z.string(),
  name: z.string(),
  path: z.string(),
  sessionIds: z.array(z.string()),
}) satisfies z.ZodType<Project>;

export const shellSchema = z.object({ id: z.string(), projectId: z.string(), number: z.number(), startedAt: z.string() }) satisfies z.ZodType<Shell>;

const todoSourceSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("app") }),
  z.object({ kind: z.literal("file"), file: z.string() }),
  z.object({ kind: z.literal("linear"), identifier: z.string(), url: z.string() }),
]);
const linearProjectSchema = z.object({ id: z.string(), name: z.string() });
const linkSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("linear"),
    teamId: z.string(),
    teamKey: z.string(),
    teamName: z.string(),
    project: linearProjectSchema.nullable(),
    label: z.string().nullable(),
  }),
]);
const connectionSchema = z.object({
  projectId: z.string(),
  link: linkSchema,
  fetchedAt: z.string().nullable(),
  error: z.string().nullable(),
  needsKey: z.boolean(),
  truncated: z.boolean(),
});

export const linearKeyStatusSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("missing") }),
  z.object({ kind: z.literal("saved"), account: z.string().nullable() }),
  z.object({ kind: z.literal("rejected") }),
  z.object({ kind: z.literal("unavailable"), reason: z.string() }),
]) satisfies z.ZodType<LinearKeyStatus>;

export const linearTeamsSchema = z.array(
  z.object({ id: z.string(), key: z.string(), name: z.string(), projects: z.array(linearProjectSchema) }),
) satisfies z.ZodType<LinearTeam[]>;

const verdict = z.enum(["pass", "concerns"]);
const reviewSchema = z.object({
  repo: z.string(),
  number: z.number(),
  title: z.string(),
  head: z.string(),
  at: z.string(),
  outcome: z.discriminatedUnion("kind", [
    z.object({ kind: z.literal("posted"), verdict, findings: z.number(), reviewId: z.number() }),
    z.object({ kind: z.literal("clean") }),
    z.object({ kind: z.literal("failed"), reason: z.string() }),
  ]),
});
const noteHold = z.enum(["busy", "waiting-on-you", "stuck", "finished", "you-typed", "awaiting-turn", "hand-started", "no-idle-signal", "paused"]);
const queuedNoteSchema = z.object({ id: z.string(), toSessionId: z.string(), text: z.string(), queuedAt: z.string(), hold: noteHold }) satisfies z.ZodType<QueuedNote>;

const managerFloorSchema = z.object({
  projectId: z.string(),
  status: z.discriminatedUnion("kind", [
    z.object({ kind: z.literal("absent") }),
    z.object({ kind: z.literal("hired"), sessionId: z.string().nullable(), startedAt: z.string(), lastCallAt: z.string().nullable() }),
  ]),
  paused: z.boolean(),
  mayHire: z.boolean(),
  escalated: z.boolean(),
  managed: z.array(z.string()),
  caps: z.object({ maxDesks: z.number(), hiredNow: z.number(), spendCeilingUsd: z.number(), spentUsd: z.number(), unpricedDesks: z.number() }),
  queue: z.array(queuedNoteSchema),
  log: z.array(
    z.object({
      id: z.string(),
      at: z.string(),
      event: z.discriminatedUnion("kind", [
        z.object({ kind: z.literal("note"), noteId: z.string(), toSessionId: z.string(), toTitle: z.string(), text: z.string(), state: z.enum(["queued", "delivered", "withdrawn", "overridden", "dropped", "failed"]) }),
        z.object({ kind: z.literal("hire"), tool: toolKind, task: z.string() }),
        z.object({ kind: z.literal("escalation"), title: z.string() }),
        z.object({ kind: z.literal("refused"), tool: z.string(), reason: z.string() }),
      ]),
    }),
  ),
});

const qaFloorSchema = z.object({
  projectId: z.string(),
  enabled: z.boolean(),
  status: z.discriminatedUnion("kind", [
    z.object({ kind: z.literal("off") }),
    z.object({ kind: z.literal("waiting") }),
    z.object({ kind: z.literal("watching"), repo: z.string(), checkedAt: z.string(), open: z.number() }),
    z.object({ kind: z.literal("blocked"), reason: z.string() }),
  ]),
  run: z.object({ number: z.number(), title: z.string(), startedAt: z.string(), sessionId: z.string().nullable() }).nullable(),
  reviews: z.array(reviewSchema),
  stamp: verdict.nullable(),
});

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
      answerVia: z.enum(["app", "terminal", "file", "store"]),
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
    z.object({ id: z.string(), projectId: z.string(), text: z.string(), done: z.boolean(), assignee: party, source: todoSourceSchema }),
  ),
  messages: z.array(
    z.object({ id: z.string(), fromSessionId: z.string(), toSessionId: z.string(), text: z.string(), sentAt: z.string() }),
  ),
  setupProjectId: z.string(),
  shells: z.array(shellSchema),
  connections: z.array(connectionSchema),
  qa: z.array(qaFloorSchema),
  managers: z.array(managerFloorSchema),
  chatDesks: z.array(z.string()),
  queued: z.array(queuedNoteSchema),
}) satisfies z.ZodType<OfficeSnapshot>;

const targetsSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("all") }),
  z.object({ kind: z.literal("only"), tools: z.array(toolKind) }),
]);
const supportSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("supported"), note: z.string().nullable() }),
  z.object({ kind: z.literal("unsupported"), reason: z.string() }),
]);
const skillInfoSchema = z.object({ name: z.string(), description: z.string() });
const itemKind = z.enum(["skill", "instructions", "mcp-server", "hook"]);

export const setupViewSchema = z.object({
  items: z.array(z.object({ kind: itemKind, name: z.string(), description: z.string(), targets: targetsSchema })),
  harnesses: z.array(
    z.object({
      tool: toolKind,
      installed: z.boolean(),
      skills: z.array(skillInfoSchema).nullable(),
      mcpServers: z.array(z.string()).nullable(),
      hooks: z.array(z.object({ id: z.string(), event: z.string(), matcher: z.string().nullable(), command: z.string() })).nullable(),
      plugins: z.array(z.object({ name: z.string(), enabled: z.boolean() })).nullable(),
      instructions: z.array(z.string()).nullable(),
      support: z.object({ skill: supportSchema, instructions: supportSchema, "mcp-server": supportSchema, hook: supportSchema }),
    }),
  ),
  problems: z.array(z.string()),
}) satisfies z.ZodType<SetupView>;

const deskView = z.enum(["terminal", "chat"]);
const chatSupportSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("available"), terminal: z.boolean() }),
  z.object({ kind: z.literal("unavailable"), reason: z.string() }),
]);

const effortSchema = z.object({ id: z.string(), label: z.string(), description: z.string() });

export const hireOptionsSchema = z.array(
  z.object({
    tool: toolKind,
    available: z.boolean(),
    models: z.array(z.object({ id: z.string(), label: z.string(), efforts: z.array(effortSchema), defaultEffort: z.string() })),
    defaultModel: z.string(),
    ultracode: z.boolean().nullable(),
    chat: chatSupportSchema,
    view: deskView,
  }),
) satisfies z.ZodType<ToolOptions[]>;

const entrySchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("user"), id: z.string(), text: z.string() }),
  z.object({ kind: z.literal("agent"), id: z.string(), text: z.string() }),
  z.object({ kind: z.literal("thought"), id: z.string(), text: z.string() }),
  z.object({ kind: z.literal("tool"), id: z.string(), title: z.string(), status: z.enum(["pending", "running", "done", "failed"]), detail: z.string(), diffs: z.array(z.object({ path: z.string(), text: z.string() })) }),
  z.object({ kind: z.literal("notice"), id: z.string(), text: z.string() }),
]);
const fieldKindSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("text") }),
  z.object({ kind: z.literal("secret") }),
  z.object({ kind: z.literal("number") }),
  z.object({ kind: z.literal("boolean") }),
  z.object({ kind: z.literal("pick"), options: z.array(z.string()), other: z.boolean() }),
]);
const approvalSchema = z.object({
  id: z.string(),
  title: z.string(),
  detail: z.string(),
  fields: z.array(z.object({ id: z.string(), label: z.string(), kind: fieldKindSchema, required: z.boolean() })),
  choices: z.array(z.object({ id: z.string(), label: z.string(), allow: z.boolean() })),
});

const capabilitiesSchema = z.object({
  compose: z.boolean(),
  stop: z.boolean(),
  end: z.boolean(),
  approvals: z.enum(["inline", "terminal", "none"]),
  terminal: z.boolean(),
});

export const conversationBacklogSchema = z.object({
  transcript: z.array(entrySchema),
  state: sessionState,
  approval: approvalSchema.nullable(),
  ended: z.string().nullable(),
  capabilities: capabilitiesSchema,
  earlier: z.boolean(),
}) satisfies z.ZodType<ConversationBacklog>;

export const conversationUpdateSchema = z.object({
  sessionId: z.string(),
  entry: entrySchema.nullable(),
  state: sessionState,
  approval: approvalSchema.nullable(),
  ended: z.string().nullable(),
  reset: z.boolean(),
}) satisfies z.ZodType<ConversationUpdate>;

export const toolUsageSchema = z.array(
  z.object({
    tool: toolKind,
    checkedAt: z.string().nullable(),
    windows: z.array(z.object({ id: z.string(), label: z.string(), percent: z.number(), resetsAt: z.string().nullable() })),
    unavailable: z.string().nullable(),
  }),
) satisfies z.ZodType<ToolUsage[]>;

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

const notificationSettingsSchema = z.object({
  finished: z.boolean(),
  needsYou: z.boolean(),
  stuck: z.boolean(),
  sound: z.boolean(),
  dockBounce: z.boolean(),
  systemNotification: z.boolean(),
  cooldownSecs: z.number(),
});

export const editorSchema = z.enum(["cursor", "vs-code", "zed", "devin-desktop"]) satisfies z.ZodType<Editor>;

export const editorListSchema = z.object({
  editors: z.array(z.object({ editor: editorSchema, label: z.string(), installed: z.boolean() })),
  defaultEditor: editorSchema.nullable(),
}) satisfies z.ZodType<EditorList>;

export const settingsSchema = z.object({
  notifications: notificationSettingsSchema,
  editor: z.object({ preferred: editorSchema.nullable() }),
  connectors: z.object({ linearAgentLabel: z.string() }),
  qa: z.object({
    defaultOn: z.boolean(),
    pollMinutes: z.number(),
    everyPush: z.boolean(),
    skipDrafts: z.boolean(),
    postWhenClean: z.boolean(),
    maxConcurrent: z.number(),
    reviewsPerDay: z.number(),
    tool: z.enum(["claude-code"]),
    model: z.string().nullable(),
    effort: z.string().nullable(),
  }),
  manager: z.object({
    tool: z.enum(["claude-code"]),
    model: z.string().nullable(),
    effort: z.string().nullable(),
    maxDesks: z.number(),
    spendCeilingUsd: z.number(),
  }),
  agents: z.object({ view: deskView }),
  routing: routingSettingsSchema,
}) satisfies z.ZodType<Settings>;

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
