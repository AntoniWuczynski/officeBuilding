//! The slice of Codex's app-server protocol the driver reads and writes, typed
//! after the bundle `codex app-server generate-ts` writes (Codex 0.161.0). The
//! wire leaves out JSON-RPC's `"jsonrpc"` field. Fields the driver does not use
//! are left out (serde skips them), so newer Codex versions still parse.

use crate::discovery::claude_code::chat::lenient;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use std::collections::BTreeMap;

/// A JSON-RPC id: Codex numbers its own requests, ours are numbers too.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum RequestId {
    Number(i64),
    Text(String),
}

/// Any line from Codex: a request (`id` and `method`), a notification
/// (`method` only) or a response (`id` with `result` or `error`). `params`
/// and `result` stay raw until the method says what they hold.
#[derive(Debug, Deserialize)]
pub(super) struct Incoming {
    pub id: Option<RequestId>,
    pub method: Option<String>,
    pub params: Option<Box<RawValue>>,
    pub result: Option<Box<RawValue>>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct RpcError {
    pub code: i64,
    pub message: String,
}

/// A value that may be absent or null.
fn nullable<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

// ---- What we send -------------------------------------------------------

#[derive(Serialize)]
pub(super) struct Request<'a, P> {
    pub id: i64,
    pub method: &'a str,
    pub params: P,
}

#[derive(Serialize)]
pub(super) struct Notification<'a> {
    pub method: &'a str,
}

#[derive(Serialize)]
pub(super) struct Reply<'a> {
    pub id: &'a RequestId,
    pub result: &'a ReplyBody,
}

#[derive(Serialize)]
pub(super) struct ErrorReply<'a> {
    pub id: &'a RequestId,
    pub error: RpcError,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InitializeParams<'a> {
    pub client_info: ClientInfo<'a>,
    pub capabilities: Capabilities,
}

#[derive(Serialize)]
pub(super) struct ClientInfo<'a> {
    pub name: &'a str,
    pub title: &'a str,
    pub version: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Capabilities {
    pub experimental_api: bool,
    pub request_attestation: bool,
}

/// `thread/resume`: reopen a thread by id; its turns come back with it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadResumeParams<'a> {
    pub thread_id: &'a str,
    pub cwd: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<&'a str>,
}

#[derive(Serialize)]
pub(super) struct ThreadStartParams<'a> {
    pub cwd: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<&'a str>,
}

/// One text input, the only kind the building sends.
#[derive(Serialize)]
#[serde(tag = "type", rename = "text")]
pub(super) struct TextInput<'a> {
    pub text: &'a str,
    pub text_elements: [(); 0],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TurnStartParams<'a> {
    pub thread_id: &'a str,
    pub input: [TextInput<'a>; 1],
    pub client_user_message_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TurnSteerParams<'a> {
    pub thread_id: &'a str,
    pub input: [TextInput<'a>; 1],
    pub client_user_message_id: &'a str,
    pub expected_turn_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TurnInterruptParams<'a> {
    pub thread_id: &'a str,
    pub turn_id: &'a str,
}

// ---- Responses to our requests -----------------------------------------

#[derive(Deserialize)]
pub(super) struct ThreadStartResult {
    pub thread: Named,
}

#[derive(Deserialize)]
pub(super) struct ThreadResumeResult {
    pub thread: ResumedThread,
}

/// A reopened thread: Codex fills `turns` only for `thread/resume` and friends.
#[derive(Deserialize)]
pub(super) struct ResumedThread {
    pub id: String,
    /// Read on its own as a `ThreadStatus`, so a shape no version has yet never fails a resume.
    #[serde(default)]
    pub status: Option<Box<RawValue>>,
    #[serde(default)]
    pub turns: Vec<ResumedTurn>,
}

#[derive(Deserialize)]
pub(super) struct ResumedTurn {
    #[serde(default)]
    pub items: Vec<ThreadItem>,
}

#[derive(Deserialize)]
pub(super) struct TurnStartResult {
    pub turn: Named,
}

#[derive(Deserialize)]
pub(super) struct Named {
    pub id: String,
}

// ---- Notifications ------------------------------------------------------

/// `turn/started` and `turn/completed`. The thread is absent only from a Codex
/// older than the schema, and is then taken for the desk's own.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TurnNote {
    #[serde(default)]
    pub thread_id: Option<String>,
    pub turn: Turn,
}

#[derive(Deserialize)]
pub(super) struct Turn {
    pub id: String,
    pub status: TurnStatus,
    pub error: Option<TurnError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum TurnStatus {
    Completed,
    Interrupted,
    Failed,
    InProgress,
    /// A status a later Codex adds, treated as `Completed`.
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TurnError {
    pub message: String,
    pub additional_details: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ItemNote {
    pub item: ThreadItem,
}

/// `item/agentMessage/delta`, `item/commandExecution/outputDelta`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeltaNote {
    pub item_id: String,
    pub delta: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SummaryDeltaNote {
    pub item_id: String,
    pub delta: String,
    pub summary_index: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SummaryPartNote {
    pub item_id: String,
    pub summary_index: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PatchUpdatedNote {
    pub item_id: String,
    pub changes: Vec<FileUpdateChange>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct McpProgressNote {
    pub item_id: String,
    pub message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PlanNote {
    pub turn_id: String,
    pub explanation: Option<String>,
    pub plan: Vec<PlanStep>,
}

#[derive(Deserialize)]
pub(super) struct PlanStep {
    pub step: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum StepStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ErrorNote {
    pub error: TurnError,
    pub will_retry: bool,
}

#[derive(Deserialize)]
pub(super) struct WarningNote {
    pub message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResolvedNote {
    pub request_id: RequestId,
}

/// `thread/status/changed`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StatusNote {
    pub thread_id: String,
    pub status: ThreadStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ThreadStatus {
    #[serde(rename_all = "camelCase")]
    Active {
        #[serde(default)]
        active_flags: Vec<ActiveFlag>,
    },
    /// `notLoaded`, `idle`, `systemError`, and any status a later Codex adds.
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ActiveFlag {
    WaitingOnApproval,
    WaitingOnUserInput,
    #[serde(other)]
    Other,
}

// ---- Thread items -------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ThreadItem {
    #[serde(rename_all = "camelCase")]
    UserMessage {
        id: String,
        client_id: Option<String>,
        content: Vec<UserInput>,
    },
    AgentMessage {
        id: String,
        text: String,
    },
    Reasoning {
        id: String,
        summary: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    CommandExecution {
        id: String,
        command: String,
        status: ItemStatus,
        #[serde(default)]
        command_actions: Vec<CommandAction>,
        aggregated_output: Option<String>,
        exit_code: Option<i64>,
    },
    FileChange {
        id: String,
        changes: Vec<FileUpdateChange>,
        status: ItemStatus,
    },
    McpToolCall {
        id: String,
        server: String,
        tool: String,
        status: ItemStatus,
        result: Option<McpResult>,
        error: Option<McpError>,
    },
    DynamicToolCall {
        id: String,
        tool: String,
        status: ItemStatus,
    },
    WebSearch {
        id: String,
        query: String,
    },
    Plan {
        id: String,
        text: String,
    },
    ContextCompaction {
        id: String,
    },
    /// A collaboration tool call (`CollabAgentToolCallThreadItem`, schema 0.161.0). Every
    /// field but the id is defaulted, so a later version's item still parses.
    #[serde(rename_all = "camelCase")]
    CollabAgentToolCall {
        id: String,
        #[serde(default)]
        tool: CollabTool,
        #[serde(default)]
        status: CollabStatus,
        #[serde(default)]
        receiver_thread_ids: Vec<String>,
        #[serde(default)]
        agents_states: BTreeMap<String, AgentState>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default, deserialize_with = "lenient")]
        reasoning_effort: Option<String>,
    },
    /// A worker's activity (`SubAgentActivityThreadItem`). 0.170.0 adds `model` and
    /// `reasoningEffort`.
    #[serde(rename_all = "camelCase")]
    SubAgentActivity {
        agent_thread_id: String,
        #[serde(default)]
        agent_path: String,
        #[serde(default)]
        kind: ActivityKind,
        #[serde(default)]
        model: Option<String>,
        #[serde(default, deserialize_with = "lenient")]
        reasoning_effort: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum CollabTool {
    SpawnAgent,
    SendInput,
    ResumeAgent,
    Wait,
    CloseAgent,
    SendMessage,
    FollowupTask,
    InterruptAgent,
    ListAgents,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum CollabStatus {
    InProgress,
    Completed,
    Failed,
    Interrupted,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct AgentState {
    #[serde(default)]
    pub status: AgentStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum AgentStatus {
    PendingInit,
    Running,
    Interrupted,
    Completed,
    Errored,
    Shutdown,
    NotFound,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ActivityKind {
    Started,
    Interacted,
    Interrupted,
    Completed,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum UserInput {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

/// Every command action carries the command it read from the shell line.
#[derive(Debug, Deserialize)]
pub(super) struct CommandAction {
    pub command: String,
}

/// Command, patch, MCP and dynamic tool statuses share these values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ItemStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
    /// A status a later Codex adds, shown as running.
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct FileUpdateChange {
    pub path: String,
    pub kind: PatchKind,
    pub diff: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum PatchKind {
    Add,
    Delete,
    Update { move_path: Option<String> },
}

#[derive(Debug, Deserialize)]
pub(super) struct McpResult {
    pub content: Vec<McpContent>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum McpContent {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub(super) struct McpError {
    pub message: String,
}

// ---- Requests from Codex ------------------------------------------------

/// Who owns a request: the thread and the turn its params name. A legacy
/// request names its thread `conversationId` and no turn. Read from every
/// request's raw params beside its typed parse.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Owner {
    #[serde(default, alias = "conversationId")]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommandApprovalParams {
    pub item_id: String,
    pub kind: Option<CommandApprovalKind>,
    pub reason: Option<String>,
    pub network_approval_context: Option<NetworkContext>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub command_actions: Option<Vec<CommandAction>>,
    pub proposed_execpolicy_amendment: Option<Vec<String>>,
    pub proposed_network_policy_amendments: Option<Vec<NetworkAmendment>>,
    /// Experimental in the schema, but Codex 0.161.0 sends it regardless.
    pub available_decisions: Option<Vec<CommandDecision>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum CommandApprovalKind {
    Command,
    WriteStdin,
    /// `humanCommand` (Codex 0.170.0) and any kind a later Codex adds: only `accept`,
    /// `decline` and `cancel` are offered for it.
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
pub(super) struct NetworkContext {
    pub host: String,
}

/// Command and file-change decisions (a file change takes the four plain ones).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum CommandDecision {
    Accept,
    AcceptForSession,
    AcceptWithExecpolicyAmendment {
        execpolicy_amendment: Vec<String>,
    },
    ApplyNetworkPolicyAmendment {
        network_policy_amendment: NetworkAmendment,
    },
    Decline,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct NetworkAmendment {
    pub host: String,
    pub action: RuleAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RuleAction {
    Allow,
    Deny,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileApprovalParams {
    pub item_id: String,
    pub reason: Option<String>,
    pub grant_root: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PermissionsParams {
    pub item_id: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    pub permissions: Permissions,
}

/// The extra permissions asked for. Each part is echoed back as it came when
/// granted, so it stays raw here and is read for display separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Permissions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Box<RawValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_system: Option<Box<RawValue>>,
}

/// Read strictly, like `FileSystemPermissions` and `FsEntry`: a field the
/// building does not know is a part of the request it cannot show, so it
/// cannot offer to grant it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NetworkPermissions {
    pub enabled: Option<bool>,
}

/// What a permissions request asks of the file system, read for display. The request
/// itself is echoed raw when granted.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FileSystemPermissions {
    #[serde(default)]
    pub entries: Option<Vec<FsEntry>>,
    #[serde(default)]
    pub glob_scan_max_depth: Option<u64>,
    #[serde(default)]
    pub read: Option<Vec<String>>,
    #[serde(default)]
    pub write: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FsEntry {
    pub access: FsAccess,
    pub path: FsPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum FsAccess {
    Read,
    Write,
    Deny,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum FsPath {
    Path {
        path: String,
    },
    GlobPattern {
        pattern: String,
    },
    Special {
        value: FsSpecial,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum FsSpecial {
    Root,
    Minimal,
    Tmpdir,
    SlashTmp,
    ProjectRoots {
        #[serde(default)]
        subpath: Option<String>,
    },
    Unknown {
        path: String,
        #[serde(default)]
        subpath: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UserInputParams {
    pub questions: Vec<Question>,
    /// Absent means blocking (`protocol/src/request_user_input.rs`, rust-v0.161.0).
    #[serde(default)]
    pub is_blocking: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Question {
    pub id: String,
    pub header: String,
    #[serde(rename = "question")]
    pub text: String,
    pub options: Option<Vec<QuestionOption>>,
    /// Any text is a fine answer besides the options.
    #[serde(default)]
    pub is_other: bool,
    #[serde(default)]
    pub is_secret: bool,
}

#[derive(Deserialize)]
pub(super) struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ElicitationHead {
    pub server_name: String,
}

#[derive(Deserialize)]
#[serde(tag = "mode")]
pub(super) enum Elicitation {
    #[serde(rename = "form")]
    Form {
        message: String,
        #[serde(rename = "requestedSchema")]
        requested_schema: FormSchema,
    },
    #[serde(rename = "url")]
    Url { message: String, url: String },
    #[serde(rename = "openai/form", alias = "openaiForm")]
    OpenaiForm { message: String },
}

#[derive(Deserialize)]
pub(super) struct FormSchema {
    #[serde(default)]
    pub properties: BTreeMap<String, FormField>,
    #[serde(default, deserialize_with = "nullable")]
    pub required: Vec<String>,
}

/// One field a form asks for (MCP elicitation's restricted JSON Schema).
#[derive(Deserialize)]
pub(super) struct FormField {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "enum")]
    pub one_of: Option<Vec<String>>,
}

/// v1 `execCommandApproval`, kept for completeness: v2 threads use
/// `item/commandExecution/requestApproval`.
#[derive(Deserialize)]
pub(super) struct LegacyExecParams {
    pub command: Vec<String>,
    pub cwd: String,
    pub reason: Option<String>,
}

/// v1 `applyPatchApproval`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LegacyPatchParams {
    pub file_changes: BTreeMap<String, LegacyFileChange>,
    pub reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum LegacyFileChange {
    Add { content: String },
    Delete,
    Update { unified_diff: String },
}

// ---- Our answers to Codex's requests -----------------------------------

/// What an approval answer sends back, chosen when the request arrives.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(super) enum ReplyBody {
    Decision {
        decision: CommandDecision,
    },
    Review {
        decision: ReviewDecision,
    },
    Permissions {
        permissions: Permissions,
        scope: GrantScope,
    },
    Input {
        answers: BTreeMap<String, Answer>,
    },
    Elicit {
        action: ElicitAction,
        content: Option<BTreeMap<String, FormValue>>,
        #[serde(rename = "_meta")]
        meta: Option<()>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReviewDecision {
    Approved,
    ApprovedForSession,
    Denied { rejection: String },
    Abort,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum GrantScope {
    Turn,
    Session,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Answer {
    pub answers: Vec<String>,
}

/// A form field's value, in the JSON type its schema asks for.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub(super) enum FormValue {
    Text(String),
    Number(serde_json::Number),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ElicitAction {
    Accept,
    Decline,
    Cancel,
}
