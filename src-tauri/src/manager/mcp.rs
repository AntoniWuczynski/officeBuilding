//! The Model Context Protocol (MCP) a manager speaks, served over a `Tools`: `initialize`,
//! `ping`, `tools/list` and `tools/call`, one JSON-RPC message per line. It always answers
//! protocol version 2025-06-18, which the specification allows.

use super::socket::Session;
use super::tools::{self, CATALOGUE, CallError, Tools};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::sync::Arc;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

const SERVER_NAME: &str = "office-building";
const INSTRUCTIONS: &str =
    "The building's tools for this floor's manager. Every call is limited to your own floor.";

const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;

/// A message from the client. Its shape is the client's, so unknown fields are let through.
#[derive(Deserialize)]
struct Incoming<'a> {
    jsonrpc: &'a str,
    #[serde(default)]
    id: Option<Id>,
    method: String,
    #[serde(borrow, default)]
    params: Option<&'a RawValue>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum Id {
    Number(i64),
    Text(String),
}

#[derive(Deserialize)]
struct CallParams<'a> {
    name: String,
    #[serde(borrow, default)]
    arguments: Option<&'a RawValue>,
}

#[derive(Serialize)]
struct Answer<'a, R: Serialize> {
    jsonrpc: &'static str,
    id: Option<&'a Id>,
    result: R,
}

#[derive(Serialize)]
struct Refusal<'a> {
    jsonrpc: &'static str,
    id: Option<&'a Id>,
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: i64,
    message: String,
}

#[derive(Serialize)]
struct Empty {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult {
    protocol_version: &'static str,
    capabilities: Capabilities,
    server_info: ServerInfo,
    instructions: &'static str,
}

#[derive(Serialize)]
struct Capabilities {
    tools: Empty,
}

#[derive(Serialize)]
struct ServerInfo {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct ToolList {
    tools: Vec<ToolEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolEntry {
    name: &'static str,
    description: &'static str,
    input_schema: &'static RawValue,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CallResult {
    content: [TextContent; 1],
    is_error: bool,
}

#[derive(Serialize)]
struct TextContent {
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
}

/// Answers a manager's MCP messages with `tools`.
pub struct Protocol<T: Tools + ?Sized> {
    tools: Arc<T>,
}

impl<T: Tools + ?Sized> Protocol<T> {
    pub fn new(tools: Arc<T>) -> Self {
        Self { tools }
    }

    /// The answer to the request `method` with `params`, or the error to send instead.
    fn answer(&self, floor: &str, method: &str, params: Option<&RawValue>) -> Reply {
        match method {
            "initialize" => Reply::of(&InitializeResult {
                protocol_version: PROTOCOL_VERSION,
                capabilities: Capabilities { tools: Empty {} },
                server_info: ServerInfo {
                    name: SERVER_NAME,
                    version: env!("CARGO_PKG_VERSION"),
                },
                instructions: INSTRUCTIONS,
            }),
            "ping" => Reply::of(&Empty {}),
            "tools/list" => catalogue().map_or_else(
                |()| Reply::Error(INTERNAL_ERROR, "internal error".to_string()),
                |tools| Reply::of(&ToolList { tools }),
            ),
            "tools/call" => self.call(floor, params),
            _ => Reply::Error(METHOD_NOT_FOUND, "method not found".to_string()),
        }
    }

    fn call(&self, floor: &str, params: Option<&RawValue>) -> Reply {
        let Some(params) =
            params.and_then(|raw| serde_json::from_str::<CallParams<'_>>(raw.get()).ok())
        else {
            return Reply::Error(INVALID_PARAMS, "invalid params".to_string());
        };
        let call = match tools::parse(&params.name, params.arguments.map(RawValue::get)) {
            Ok(call) => call,
            Err(CallError::UnknownTool) => {
                return Reply::Error(INVALID_PARAMS, format!("unknown tool: {}", params.name));
            }
            Err(CallError::BadArguments(message)) => return Reply::Error(INVALID_PARAMS, message),
        };
        let (text, is_error) = match self.tools.call(floor, call) {
            Ok(text) => (text, false),
            Err(reason) => (reason, true),
        };
        Reply::of(&CallResult {
            content: [TextContent { kind: "text", text }],
            is_error,
        })
    }
}

/// A request's outcome before it is wrapped with the request's id.
enum Reply {
    /// The result, already serialised.
    Result(Box<RawValue>),
    Error(i64, String),
}

impl Reply {
    fn of<R: Serialize>(result: &R) -> Self {
        serde_json::value::to_raw_value(result).map_or_else(
            |_| Self::Error(INTERNAL_ERROR, "internal error".to_string()),
            Self::Result,
        )
    }
}

/// The tools of the catalogue, each schema embedded as JSON. Err: a schema is not JSON.
fn catalogue() -> Result<Vec<ToolEntry>, ()> {
    CATALOGUE
        .iter()
        .map(|tool| {
            serde_json::from_str::<&'static RawValue>(tool.schema)
                .map(|input_schema| ToolEntry {
                    name: tool.name,
                    description: tool.description,
                    input_schema,
                })
                .map_err(drop)
        })
        .collect()
}

fn error_line(id: Option<&Id>, code: i64, message: String) -> Option<String> {
    serde_json::to_string(&Refusal {
        jsonrpc: "2.0",
        id,
        error: ErrorBody { code, message },
    })
    .ok()
}

impl<T: Tools + ?Sized> Session for Protocol<T> {
    fn handle(&self, floor: &str, line: &str) -> Option<String> {
        // A batch or any other non-object is not a request this server takes.
        let incoming = line
            .trim_start()
            .starts_with('{')
            .then(|| serde_json::from_str::<Incoming<'_>>(line).ok())
            .flatten()
            .filter(|incoming| incoming.jsonrpc == "2.0");
        let Some(incoming) = incoming else {
            return error_line(None, INVALID_REQUEST, "invalid request".to_string());
        };
        // A notification gets no reply, whatever it is.
        let id = incoming.id.as_ref()?;
        match self.answer(floor, &incoming.method, incoming.params) {
            Reply::Result(result) => serde_json::to_string(&Answer {
                jsonrpc: "2.0",
                id: Some(id),
                result,
            })
            .ok(),
            Reply::Error(code, message) => error_line(Some(id), code, message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::auth::Registry;
    use crate::manager::helper::relay;
    use crate::manager::socket::Server;
    use crate::manager::socket::tests::short_dir;
    use crate::manager::tools::{Call, ReadDesk};
    use std::io::Cursor;
    use std::os::unix::net::UnixStream;
    use std::sync::Mutex;

    /// Records each call and answers `read_desk` with an error, everything else with "ok".
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<(String, Call)>>,
    }

    impl Tools for Fake {
        fn call(&self, floor: &str, call: Call) -> Result<String, String> {
            crate::sync::lock(&self.calls).push((floor.to_string(), call.clone()));
            match call {
                Call::ReadDesk(_) => Err("there is no such desk on your floor".to_string()),
                _ => Ok(r#"{"ok":true}"#.to_string()),
            }
        }
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(untagged)]
    enum TestId {
        Number(i64),
        Text(String),
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestReply<R> {
        jsonrpc: String,
        id: Option<TestId>,
        result: Option<R>,
        error: Option<TestError>,
    }

    #[derive(Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct TestError {
        code: i64,
        message: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestEmpty {}

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestInit {
        protocol_version: String,
        capabilities: TestCaps,
        server_info: TestInfo,
        instructions: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestCaps {
        tools: TestEmpty,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestInfo {
        name: String,
        version: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestList {
        tools: Vec<TestTool>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestTool {
        name: String,
        description: String,
        input_schema: Box<RawValue>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestCall {
        content: Vec<TestContent>,
        is_error: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestContent {
        #[serde(rename = "type")]
        kind: String,
        text: String,
    }

    fn protocol() -> (Protocol<Fake>, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        (Protocol::new(Arc::clone(&fake)), fake)
    }

    fn reply<R: serde::de::DeserializeOwned>(
        protocol: &Protocol<Fake>,
        line: &str,
    ) -> TestReply<R> {
        let text = protocol.handle("p-1", line).unwrap();
        assert!(!text.contains('\n'), "one line");
        let reply: TestReply<R> = serde_json::from_str(&text).unwrap();
        assert_eq!(reply.jsonrpc, "2.0");
        reply
    }

    fn error(protocol: &Protocol<Fake>, line: &str) -> (Option<TestId>, TestError) {
        let reply = reply::<TestEmpty>(protocol, line);
        assert!(reply.result.is_none());
        (reply.id, reply.error.unwrap())
    }

    fn call(protocol: &Protocol<Fake>, name: &str, arguments: &str) -> TestCall {
        let line = format!(
            r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"{name}","arguments":{arguments}}}}}"#
        );
        let reply = reply::<TestCall>(protocol, &line);
        assert!(reply.error.is_none());
        reply.result.unwrap()
    }

    #[test]
    fn initialize_answers_its_own_version_whatever_is_asked() {
        let (protocol, _) = protocol();
        for asked in ["2025-06-18", "2024-11-05", "2099-01-01"] {
            let line = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{asked}","capabilities":{{}},"clientInfo":{{"name":"claude-code","version":"2"}}}}}}"#
            );
            let reply = reply::<TestInit>(&protocol, &line);
            assert_eq!(reply.id, Some(TestId::Number(1)));
            let init = reply.result.unwrap();
            assert_eq!(init.protocol_version, "2025-06-18");
            let TestCaps {
                tools: TestEmpty {},
            } = init.capabilities;
            assert_eq!(init.server_info.name, "office-building");
            assert_eq!(init.server_info.version, env!("CARGO_PKG_VERSION"));
            assert!(init.instructions.contains("your own floor"));
        }
    }

    #[test]
    fn notifications_get_no_reply_even_unknown_ones() {
        let (protocol, fake) = protocol();
        for line in [
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3}}"#,
            r#"{"jsonrpc":"2.0","method":"no/such/thing"}"#,
            r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"list_todos"}}"#,
        ] {
            assert_eq!(protocol.handle("p-1", line), None, "{line}");
        }
        assert!(
            crate::sync::lock(&fake.calls).is_empty(),
            "a notification runs nothing"
        );
    }

    #[test]
    fn ping_gets_an_empty_object() {
        let (protocol, _) = protocol();
        let reply = reply::<TestEmpty>(&protocol, r#"{"jsonrpc":"2.0","id":"p","method":"ping"}"#);
        assert!(reply.result.is_some() && reply.error.is_none());
        assert_eq!(
            protocol.handle("p-1", r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#),
            Some(r#"{"jsonrpc":"2.0","id":2,"result":{}}"#.to_string())
        );
    }

    #[test]
    fn tools_list_holds_the_seven_tools_with_object_schemas() {
        let (protocol, _) = protocol();
        let reply = reply::<TestList>(
            &protocol,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}"#,
        );
        let tools = reply.result.unwrap().tools;
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        let expected: Vec<&str> = CATALOGUE.iter().map(|t| t.name).collect();
        assert_eq!(names, expected);
        assert_eq!(tools.len(), 7);
        for tool in &tools {
            assert!(tool.input_schema.get().starts_with('{'), "{}", tool.name);
            assert!(!tool.description.is_empty());
        }
    }

    #[test]
    fn tools_call_hands_the_floor_and_the_parsed_call_and_wraps_the_answer() {
        let (protocol, fake) = protocol();
        let done = call(&protocol, "list_todos", "{}");
        assert!(!done.is_error);
        assert_eq!(done.content.len(), 1);
        assert_eq!(done.content[0].kind, "text");
        assert_eq!(done.content[0].text, r#"{"ok":true}"#);

        let refused = call(&protocol, "read_desk", r#"{"desk":"d-1","lines":3}"#);
        assert!(refused.is_error);
        assert_eq!(
            refused.content[0].text,
            "there is no such desk on your floor"
        );

        // No arguments at all is read as none.
        let line =
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"list_desks"}}"#;
        assert!(!reply::<TestCall>(&protocol, line).result.unwrap().is_error);

        assert_eq!(
            *crate::sync::lock(&fake.calls),
            [
                ("p-1".to_string(), Call::ListTodos),
                (
                    "p-1".to_string(),
                    Call::ReadDesk(ReadDesk {
                        desk: "d-1".to_string(),
                        lines: Some(3)
                    })
                ),
                ("p-1".to_string(), Call::ListDesks),
            ]
        );
    }

    #[test]
    fn unknown_tools_and_bad_arguments_are_invalid_params() {
        let (protocol, fake) = protocol();
        let (id, e) = error(
            &protocol,
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"stop_desk","arguments":{}}}"#,
        );
        assert_eq!(id, Some(TestId::Number(7)));
        assert_eq!(
            (e.code, e.message.as_str()),
            (-32602, "unknown tool: stop_desk")
        );
        let (_, e) = error(
            &protocol,
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"read_desk","arguments":{"desk":"d","floor":"p-2"}}}"#,
        );
        assert_eq!(e.code, -32602);
        assert!(e.message.contains("floor"), "{}", e.message);
        for params in [
            "",
            r#","params":{}"#,
            r#","params":{"name":7}"#,
            r#","params":null"#,
        ] {
            let line = format!(r#"{{"jsonrpc":"2.0","id":9,"method":"tools/call"{params}}}"#);
            let (_, e) = error(&protocol, &line);
            assert_eq!(
                (e.code, e.message.as_str()),
                (-32602, "invalid params"),
                "{line}"
            );
        }
        assert!(crate::sync::lock(&fake.calls).is_empty());
    }

    #[test]
    fn an_unknown_method_is_not_found() {
        let (protocol, _) = protocol();
        let (id, e) = error(
            &protocol,
            r#"{"jsonrpc":"2.0","id":10,"method":"resources/list"}"#,
        );
        assert_eq!(id, Some(TestId::Number(10)));
        assert_eq!((e.code, e.message.as_str()), (-32601, "method not found"));
    }

    #[test]
    fn garbage_and_batches_are_invalid_requests_with_a_null_id() {
        let (protocol, _) = protocol();
        for line in [
            "not json",
            "",
            "42",
            r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#,
            r#"["2.0",1,"ping"]"#,
            r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
            r#"{"id":1,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":1}"#,
            r#"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#,
        ] {
            let (id, e) = error(&protocol, line);
            assert_eq!(id, None, "{line}");
            assert_eq!(
                (e.code, e.message.as_str()),
                (-32600, "invalid request"),
                "{line}"
            );
        }
        let raw = protocol.handle("p-1", "not json").unwrap();
        assert!(raw.contains(r#""id":null"#), "{raw}");
    }

    #[test]
    fn number_and_string_ids_come_back_unchanged() {
        let (protocol, _) = protocol();
        for (id, expected) in [
            ("0", TestId::Number(0)),
            ("-4", TestId::Number(-4)),
            ("9007199254740993", TestId::Number(9_007_199_254_740_993)),
            (r#""abc-1""#, TestId::Text("abc-1".to_string())),
            (r#""""#, TestId::Text(String::new())),
        ] {
            let line = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#);
            assert_eq!(reply::<TestEmpty>(&protocol, &line).id, Some(expected));
        }
    }

    #[test]
    fn end_to_end_through_the_socket_and_the_helper() {
        let base = short_dir("mc");
        let server = Server::bind(&base.join("ipc")).unwrap();
        let path = server.path().to_path_buf();
        let registry = Arc::new(Registry::default());
        let token = registry.admit("p-7").unwrap();
        let fake = Arc::new(Fake::default());
        server
            .serve(
                Arc::clone(&registry),
                Arc::new(Protocol::new(Arc::clone(&fake))),
            )
            .unwrap();
        let input = [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_todos","arguments":{}}}"#,
        ]
        .map(|l| format!("{l}\n"))
        .concat();
        let mut output = Vec::new();
        relay(
            UnixStream::connect(&path).unwrap(),
            token.expose(),
            Cursor::new(input.into_bytes()),
            &mut output,
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3, "the notification has no reply");
        let init: TestReply<TestInit> = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(init.result.unwrap().protocol_version, PROTOCOL_VERSION);
        let list: TestReply<TestList> = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(list.result.unwrap().tools.len(), 7);
        let called: TestReply<TestCall> = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(called.id, Some(TestId::Number(3)));
        assert!(!called.result.unwrap().is_error);
        assert_eq!(
            *crate::sync::lock(&fake.calls),
            [("p-7".to_string(), Call::ListTodos)]
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
