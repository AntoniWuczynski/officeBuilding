//! The JSON-RPC envelope and the ACP messages the driver reads and writes, as
//! typed serde structs (field names from `schema/v1/schema.json`). Anything the
//! driver does not act on is left out; unknown variants land in `Other`.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// `-32601`: the method does not exist (JSON-RPC 2.0).
pub(super) const METHOD_NOT_FOUND: i64 = -32601;
/// `-32800`: the request was cancelled (ACP `ErrorCode`).
pub(super) const REQUEST_CANCELLED: i64 = -32800;
/// `-32000`: the agent wants the human signed in (ACP `ErrorCode`).
pub(super) const AUTH_REQUIRED: i64 = -32000;

/// A JSON-RPC request id. The agent's own requests are answered with the id
/// exactly as sent (opencode numbers them from 0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum Id {
    Number(i64),
    Text(String),
}

/// One line from the agent before it is sorted into a response, a request or a
/// notification.
#[derive(Debug, Deserialize)]
struct Envelope {
    id: Option<Id>,
    method: Option<String>,
    params: Option<Box<RawValue>>,
    result: Option<Box<RawValue>>,
    error: Option<RpcError>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Box<RawValue>>,
}

impl RpcError {
    /// The message, or the longer one Cursor puts in `data.message`.
    pub(super) fn text(&self) -> String {
        #[derive(Deserialize)]
        struct Data {
            message: String,
        }
        self.data
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Data>(raw.get()).ok())
            .map_or_else(|| self.message.clone(), |d| d.message)
    }
}

/// A line from the agent, sorted.
#[derive(Debug)]
pub(super) enum Inbound {
    /// The answer to one of our requests (`result` is `None` for `null`).
    Response {
        id: u64,
        reply: Result<Option<Box<RawValue>>, RpcError>,
    },
    Request {
        id: Id,
        method: String,
        params: Option<Box<RawValue>>,
    },
    Notification {
        method: String,
        params: Option<Box<RawValue>>,
    },
}

/// Sort one stdout line. `None` for anything that is not JSON-RPC (a login
/// shell's greeting, a stray log line) or a response to an id we never sent.
pub(super) fn parse(line: &str) -> Option<Inbound> {
    let env: Envelope = serde_json::from_str(line).ok()?;
    match (env.method, env.id) {
        (Some(method), Some(id)) => Some(Inbound::Request {
            id,
            method,
            params: env.params,
        }),
        (Some(method), None) => Some(Inbound::Notification {
            method,
            params: env.params,
        }),
        (None, Some(Id::Number(n))) => Some(Inbound::Response {
            id: u64::try_from(n).ok()?,
            reply: env.error.map_or(Ok(env.result), Err),
        }),
        (None, _) => None,
    }
}

/// Parse `params` (or a result) into `T`; a missing body reads as `null`.
pub(super) fn body<T: for<'de> Deserialize<'de>>(raw: Option<&RawValue>) -> Option<T> {
    serde_json::from_str(raw.map_or("null", RawValue::get)).ok()
}

#[derive(Serialize)]
struct OutRequest<'a, P> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: P,
}

#[derive(Serialize)]
struct OutNotification<'a, P> {
    jsonrpc: &'static str,
    method: &'a str,
    params: P,
}

#[derive(Serialize)]
struct OutResult<'a, R> {
    jsonrpc: &'static str,
    id: &'a Id,
    result: R,
}

#[derive(Serialize)]
struct OutError<'a> {
    jsonrpc: &'static str,
    id: &'a Id,
    error: OutErrorBody<'a>,
}

#[derive(Serialize)]
struct OutErrorBody<'a> {
    code: i64,
    message: &'a str,
}

fn line<T: Serialize>(msg: &T) -> Result<String, String> {
    serde_json::to_string(msg).map_err(|e| format!("could not write a message for the agent: {e}"))
}

pub(super) fn request<P: Serialize>(id: u64, method: &str, params: P) -> Result<String, String> {
    line(&OutRequest {
        jsonrpc: "2.0",
        id,
        method,
        params,
    })
}

pub(super) fn notification<P: Serialize>(method: &str, params: P) -> Result<String, String> {
    line(&OutNotification {
        jsonrpc: "2.0",
        method,
        params,
    })
}

pub(super) fn result<R: Serialize>(id: &Id, result: R) -> Result<String, String> {
    line(&OutResult {
        jsonrpc: "2.0",
        id,
        result,
    })
}

pub(super) fn error(id: &Id, code: i64, message: &str) -> Result<String, String> {
    line(&OutError {
        jsonrpc: "2.0",
        id,
        error: OutErrorBody { code, message },
    })
}

// --- initialize -------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InitializeParams {
    pub protocol_version: u16,
    pub client_capabilities: ClientCapabilities,
    pub client_info: Implementation,
}

/// No `fs` and no `terminal`: the agent reads, writes and runs commands with
/// its own tools, and the building only shows what it did.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ClientCapabilities {
    pub fs: FileSystemCapabilities,
    pub terminal: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileSystemCapabilities {
    pub read_text_file: bool,
    pub write_text_file: bool,
}

#[derive(Serialize)]
pub(super) struct Implementation {
    pub name: &'static str,
    pub title: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InitializeResult {
    pub protocol_version: u16,
    #[serde(default)]
    pub auth_methods: Vec<AuthMethod>,
    #[serde(default)]
    pub agent_capabilities: AgentCapabilities,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AgentCapabilities {
    /// It can reopen a session with `session/load`, replaying it as updates.
    #[serde(default)]
    pub load_session: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AuthMethod {
    pub name: String,
    pub description: Option<String>,
}

// --- session/new and model choice -------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NewSessionParams<'a> {
    pub cwd: &'a str,
    pub mcp_servers: [(); 0],
}

/// `session/load`: reopen a session by id in `cwd`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoadSessionParams<'a> {
    pub session_id: &'a str,
    pub cwd: &'a str,
    pub mcp_servers: [(); 0],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NewSessionResult {
    pub session_id: String,
}

/// The model pickers a new session can offer, read apart from the session id
/// so an odd picker never loses the session.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelChoices {
    #[serde(default)]
    pub config_options: Option<Vec<ConfigOption>>,
    /// The `models` field of the unstable session-model API, removed from the
    /// schema in agent-client-protocol#1325 but still what Cursor sends.
    #[serde(default)]
    pub models: Option<ModelState>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ConfigOption {
    Select(SelectConfig),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub(super) struct SelectConfig {
    pub id: String,
    pub category: Option<String>,
    pub options: SelectValues,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum SelectValues {
    Flat(Vec<SelectValue>),
    Grouped(Vec<SelectGroup>),
}

#[derive(Debug, Deserialize)]
pub(super) struct SelectGroup {
    pub options: Vec<SelectValue>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SelectValue {
    pub value: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelState {
    pub available_models: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelInfo {
    pub model_id: String,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetConfigOptionParams<'a> {
    pub session_id: &'a str,
    pub config_id: &'a str,
    pub value: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetModelParams<'a> {
    pub session_id: &'a str,
    pub model_id: &'a str,
}

// --- prompt turn ------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PromptParams<'a> {
    pub session_id: &'a str,
    pub prompt: [TextBlock<'a>; 1],
}

#[derive(Serialize)]
pub(super) struct TextBlock<'a> {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PromptResult {
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CancelParams<'a> {
    pub session_id: &'a str,
}

// --- session/update ---------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(super) struct SessionNotification {
    update: Box<RawValue>,
}

/// The `sessionUpdate` tag alone.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTag {
    session_update: String,
}

impl SessionNotification {
    /// The update, read by its tag in two steps: serde's internally tagged
    /// enums buffer their input, and a buffered `RawValue` (`rawInput`) fails.
    pub(super) fn update(&self) -> Option<SessionUpdate> {
        fn parse<T: for<'de> Deserialize<'de>>(raw: &str) -> Option<T> {
            serde_json::from_str(raw).ok()
        }
        let raw = self.update.get();
        let tag: UpdateTag = parse(raw)?;
        Some(match tag.session_update.as_str() {
            "user_message_chunk" => SessionUpdate::UserMessageChunk(parse(raw)?),
            "agent_message_chunk" => SessionUpdate::AgentMessageChunk(parse(raw)?),
            "agent_thought_chunk" => SessionUpdate::AgentThoughtChunk(parse(raw)?),
            "tool_call" | "tool_call_update" => SessionUpdate::ToolCall(parse(raw)?),
            "plan" => SessionUpdate::Plan(parse(raw)?),
            _ => SessionUpdate::Other,
        })
    }
}

#[derive(Debug)]
pub(super) enum SessionUpdate {
    UserMessageChunk(Chunk),
    AgentMessageChunk(Chunk),
    AgentThoughtChunk(Chunk),
    /// `tool_call` or `tool_call_update`: an update sends only what changed.
    ToolCall(ToolCallFields),
    Plan(Plan),
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Chunk {
    pub content: ContentBlock,
    pub message_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ContentBlock {
    Text {
        text: String,
    },
    ResourceLink {
        name: String,
        uri: String,
    },
    #[serde(other)]
    Other,
}

/// `ToolCall` and `ToolCallUpdate` share their fields; an update sends only
/// the ones that changed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolCallFields {
    pub tool_call_id: String,
    pub title: Option<String>,
    pub status: Option<ToolCallStatus>,
    pub content: Option<Vec<ToolCallContent>>,
    /// Shown as-is when a permission request has nothing better to show.
    pub raw_input: Option<Box<RawValue>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ToolCallContent {
    Content {
        content: ContentBlock,
    },
    #[serde(rename_all = "camelCase")]
    Diff {
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub(super) struct Plan {
    pub entries: Vec<PlanEntry>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlanEntry {
    pub content: String,
    pub status: PlanStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PlanStatus {
    Pending,
    InProgress,
    Completed,
}

// --- session/request_permission ---------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PermissionRequest {
    pub tool_call: ToolCallFields,
    pub options: Vec<PermissionOption>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: PermissionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PermissionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

#[derive(Serialize)]
pub(super) struct PermissionResult<'a> {
    pub outcome: PermissionOutcome<'a>,
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub(super) enum PermissionOutcome<'a> {
    #[serde(rename_all = "camelCase")]
    Selected {
        option_id: &'a str,
    },
    Cancelled,
}

/// `$/cancel_request`: the agent withdraws one of its requests.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CancelRequest {
    pub request_id: Id,
}
