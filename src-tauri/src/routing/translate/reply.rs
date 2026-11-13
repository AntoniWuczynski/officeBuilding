//! The reply: `OpenAI`'s Responses stream turned into Anthropic's Messages stream (section 7).
//! Pure: one event's data in, the bytes for the client out. No socket, no clock, no state but
//! the reducer's own. Hidden reasoning is dropped here and leaves no trace (A Human's ruling).

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::sse;
use crate::routing::body::{Object, Typed};

// OpenAI's events, as read. The type is probed first (`body::Typed`), then the one struct for
// it is read. None has `deny_unknown_fields`: `OpenAI` lists new properties as a compatible
// change (U9), and only the members below are relied on, each from U2 or U3.

/// `response.created` (U2).
#[derive(Deserialize)]
struct Created {
    response: Object<Head>,
}

#[derive(Deserialize)]
struct Head {
    id: String,
    model: String,
}

/// `response.output_item.added` and `.done` (U2, U3).
#[derive(Deserialize)]
struct ItemEvent {
    output_index: u32,
    item: Object<Item>,
}

#[derive(Deserialize)]
struct Item {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// `response.content_part.added` and `.done` (U2).
#[derive(Deserialize)]
struct PartEvent {
    output_index: u32,
    content_index: u32,
    #[serde(default)]
    part: Option<Object<PartKind>>,
}

#[derive(Deserialize)]
struct PartKind {
    #[serde(rename = "type")]
    kind: String,
}

/// `response.output_text.delta` and `response.refusal.delta` (U2).
#[derive(Deserialize)]
struct TextDelta<'a> {
    output_index: u32,
    content_index: u32,
    #[serde(borrow)]
    delta: Cow<'a, str>,
}

/// `response.function_call_arguments.delta` (U2, U3).
#[derive(Deserialize)]
struct ArgsDelta<'a> {
    output_index: u32,
    #[serde(borrow)]
    delta: Cow<'a, str>,
}

/// `response.completed`, `response.incomplete` and `response.failed` (U2).
#[derive(Deserialize)]
struct Terminal {
    response: Object<Tail>,
}

#[derive(Deserialize)]
struct Tail {
    #[serde(default)]
    error: Option<Object<Fault>>,
    #[serde(default)]
    incomplete_details: Option<Object<Why>>,
    #[serde(default)]
    usage: Option<Object<Usage>>,
}

/// `response.error`, and the `error` event, which has the same two members (U2).
#[derive(Deserialize)]
struct Fault {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct Why {
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    input_tokens_details: Option<Object<InputDetails>>,
}

#[derive(Deserialize)]
struct InputDetails {
    #[serde(default)]
    cached_tokens: u64,
    #[serde(default)]
    cache_write_tokens: u64,
}

// Anthropic's events, as written (U11).

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Out<'a> {
    MessageStart {
        message: Message<'a>,
    },
    ContentBlockStart {
        index: u32,
        content_block: Block<'a>,
    },
    ContentBlockDelta {
        index: u32,
        delta: Delta<'a>,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: Stop,
        usage: Used,
    },
    MessageStop,
}

impl Out<'_> {
    /// The SSE event name, which is the `type` in the data (U11).
    fn name(&self) -> &'static str {
        match self {
            Out::MessageStart { .. } => "message_start",
            Out::ContentBlockStart { .. } => "content_block_start",
            Out::ContentBlockDelta { .. } => "content_block_delta",
            Out::ContentBlockStop { .. } => "content_block_stop",
            Out::MessageDelta { .. } => "message_delta",
            Out::MessageStop => "message_stop",
        }
    }
}

/// A Message with empty `content` and null stop fields (U11).
#[derive(Serialize)]
struct Message<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    role: &'static str,
    content: [(); 0],
    model: &'a str,
    stop_reason: Option<&'static str>,
    stop_sequence: Option<&'static str>,
    usage: Used,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block<'a> {
    /// Starts as `{"type":"text","text":""}` (U11).
    Text { text: &'static str },
    /// Starts as `{"type":"tool_use","id":…,"name":…,"input":{}}` (U11).
    ToolUse {
        id: &'a str,
        name: &'a str,
        input: Empty,
    },
}

#[derive(Serialize)]
struct Empty {}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Delta<'a> {
    TextDelta { text: &'a str },
    InputJsonDelta { partial_json: &'a str },
}

#[derive(Serialize)]
struct Stop {
    stop_reason: &'static str,
    stop_sequence: Option<&'static str>,
}

#[derive(Serialize)]
#[expect(
    clippy::struct_field_names,
    reason = "the names are Anthropic's on the wire (U10)"
)]
struct Used {
    #[serde(skip_serializing_if = "Option::is_none")]
    input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_read_input_tokens: Option<u64>,
    output_tokens: u64,
}

impl Used {
    /// F16: Anthropic's total input is the sum of the three (U10), `OpenAI`'s two cache numbers
    /// are a breakdown of its `input_tokens` (U2). No usage: `output_tokens: 0` alone.
    fn from(usage: Option<Usage>) -> Self {
        let Some(usage) = usage else {
            return Self {
                input_tokens: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                output_tokens: 0,
            };
        };
        let (read, written) = usage
            .input_tokens_details
            .map_or((0, 0), |Object(d)| (d.cached_tokens, d.cache_write_tokens));
        Self {
            input_tokens: Some(
                usage
                    .input_tokens
                    .saturating_sub(read)
                    .saturating_sub(written),
            ),
            cache_creation_input_tokens: Some(written),
            cache_read_input_tokens: Some(read),
            output_tokens: usage.output_tokens,
        }
    }
}

/// Why a reply could not be finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// `response.failed`, with `response.error`'s code and message when it has them.
    Failed {
        code: Option<String>,
        message: Option<String>,
    },
    /// The `error` event.
    Error {
        code: Option<String>,
        message: Option<String>,
    },
    /// `response.incomplete` for a reason this route has no stop reason for (F17).
    Incomplete { reason: Option<String> },
    /// An event that does not read as its documented shape or comes out of order, or output
    /// this route cannot carry.
    Unreadable,
}

#[derive(Debug, PartialEq, Eq)]
pub enum End {
    /// The reply is whole: `message_stop` is in `out`.
    Clean,
    Failed(Failure),
}

/// What one upstream event came to.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// Bytes for the client, possibly none.
    pub out: Vec<u8>,
    pub end: Option<End>,
}

/// Which upstream item or part an open Anthropic block stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Text { output: u32, content: u32 },
    Call { output: u32 },
}

struct Open {
    key: Key,
    index: u32,
    /// For a call: whether an argument delta has been passed on.
    had_delta: bool,
}

/// The reducer from `OpenAI`'s stream events to Anthropic's.
#[derive(Default)]
pub struct Reply {
    /// The reply's `id` and `model`, from `response.created` (F15).
    head: Option<Head>,
    /// Whether `message_start` has been written.
    started: bool,
    /// The open blocks, in index order.
    open: Vec<Open>,
    /// The next free Anthropic `index`.
    next: u32,
    /// Whether any tool block was opened.
    called: bool,
}

impl Reply {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Take one event's data. After a step with an `end`, it is not called again.
    /// A step that fails writes no bytes of its own.
    pub fn take(&mut self, data: &[u8]) -> Step {
        let mut out = Vec::new();
        match self.apply(data, &mut out) {
            Ok(end) => Step { out, end },
            Err(failure) => Step {
                out: Vec::new(),
                end: Some(End::Failed(failure)),
            },
        }
    }

    fn apply(&mut self, data: &[u8], out: &mut Vec<u8>) -> Result<Option<End>, Failure> {
        let Typed { kind } = read::<Typed<'_>>(data)?;
        match kind.as_ref() {
            "response.created" => {
                let Created {
                    response: Object(head),
                } = read(data)?;
                self.head = Some(head);
            }
            "response.output_item.added" => self.item_added(read(data)?, out)?,
            "response.output_item.done" => self.item_done(read(data)?, out)?,
            "response.content_part.added" => Self::part_added(&read(data)?)?,
            "response.content_part.done" => {
                let part: PartEvent = read(data)?;
                let key = Key::Text {
                    output: part.output_index,
                    content: part.content_index,
                };
                self.close_where(out, |k| k == key)?;
            }
            "response.output_text.delta" | "response.refusal.delta" => {
                self.text_delta(&read(data)?, out)?;
            }
            "response.function_call_arguments.delta" => self.args_delta(&read(data)?, out)?,
            "response.completed" => {
                // A call with no `output_item.done` may hold cut-short arguments: never a clean end.
                if self.open.iter().any(|o| matches!(o.key, Key::Call { .. })) {
                    return Err(Failure::Unreadable);
                }
                let Terminal {
                    response: Object(tail),
                } = read(data)?;
                let stop = if self.called { "tool_use" } else { "end_turn" };
                self.finish(stop, tail.usage, out)?;
                return Ok(Some(End::Clean));
            }
            "response.incomplete" => {
                let Terminal {
                    response: Object(tail),
                } = read(data)?;
                let reason = tail.incomplete_details.and_then(|Object(why)| why.reason);
                let Some(stop) = reason.as_deref().and_then(incomplete_stop) else {
                    return Err(Failure::Incomplete { reason });
                };
                self.finish(stop, tail.usage, out)?;
                return Ok(Some(End::Clean));
            }
            "response.failed" => {
                let Terminal {
                    response: Object(tail),
                } = read(data)?;
                let (code, message) = tail
                    .error
                    .map(|Object(f)| (f.code, f.message))
                    .unwrap_or_default();
                return Err(Failure::Failed { code, message });
            }
            "error" => {
                let Fault { code, message } = read(data)?;
                return Err(Failure::Error { code, message });
            }
            // `response.in_progress`, `response.queued`, the `.done` events whose content has
            // already passed as deltas, every `response.reasoning_…` event, and any type this
            // route does not know (U9, F18).
            _ => {}
        }
        Ok(None)
    }

    fn item_added(&mut self, event: ItemEvent, out: &mut Vec<u8>) -> Result<(), Failure> {
        let Object(item) = event.item;
        match item.kind.as_str() {
            "function_call" => self.open_call(event.output_index, &item, out),
            // A reasoning item is hidden reasoning: dropped here.
            "message" | "reasoning" => Ok(()),
            _ => Err(Failure::Unreadable),
        }
    }

    fn item_done(&mut self, event: ItemEvent, out: &mut Vec<u8>) -> Result<(), Failure> {
        let output = event.output_index;
        let Object(item) = event.item;
        match item.kind.as_str() {
            "function_call" => {
                let key = Key::Call { output };
                if self.find(key).is_none() {
                    self.open_call(output, &item, out)?;
                }
                let Some(open) = self.find(key) else {
                    return Err(Failure::Unreadable);
                };
                if !open.had_delta {
                    // U11: the final input is always an object.
                    let args = item.arguments.as_deref().filter(|a| !a.is_empty());
                    let delta = Delta::InputJsonDelta {
                        partial_json: args.unwrap_or("{}"),
                    };
                    let index = open.index;
                    emit(out, &Out::ContentBlockDelta { index, delta })?;
                }
                self.close_where(out, |k| k == key)
            }
            "message" => self.close_where(
                out,
                |k| matches!(k, Key::Text { output: o, .. } if o == output),
            ),
            "reasoning" => Ok(()),
            _ => Err(Failure::Unreadable),
        }
    }

    fn part_added(event: &PartEvent) -> Result<(), Failure> {
        let Some(Object(part)) = &event.part else {
            return Err(Failure::Unreadable);
        };
        match part.kind.as_str() {
            // The block opens on the first delta, so nothing is written before model output.
            "output_text" | "refusal" | "reasoning_text" => Ok(()),
            _ => Err(Failure::Unreadable),
        }
    }

    fn text_delta(&mut self, event: &TextDelta<'_>, out: &mut Vec<u8>) -> Result<(), Failure> {
        if event.delta.is_empty() {
            return Ok(());
        }
        let index = self.open_text(event.output_index, event.content_index, out)?;
        let delta = Delta::TextDelta { text: &event.delta };
        emit(out, &Out::ContentBlockDelta { index, delta })
    }

    fn args_delta(&mut self, event: &ArgsDelta<'_>, out: &mut Vec<u8>) -> Result<(), Failure> {
        if event.delta.is_empty() {
            return Ok(());
        }
        let key = Key::Call {
            output: event.output_index,
        };
        let open = self
            .open
            .iter_mut()
            .find(|o| o.key == key)
            .ok_or(Failure::Unreadable)?;
        open.had_delta = true;
        let index = open.index;
        let delta = Delta::InputJsonDelta {
            partial_json: &event.delta,
        };
        emit(out, &Out::ContentBlockDelta { index, delta })
    }

    /// Writes `message_start` once, with `OpenAI`'s own `id` and `model` (F15).
    fn start(&mut self, out: &mut Vec<u8>) -> Result<(), Failure> {
        if self.started {
            return Ok(());
        }
        let head = self.head.as_ref().ok_or(Failure::Unreadable)?;
        let message = Message {
            id: &head.id,
            kind: "message",
            role: "assistant",
            content: [],
            model: &head.model,
            stop_reason: None,
            stop_sequence: None,
            usage: Used {
                input_tokens: Some(0),
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                output_tokens: 0,
            },
        };
        emit(out, &Out::MessageStart { message })?;
        self.started = true;
        Ok(())
    }

    fn find(&self, key: Key) -> Option<&Open> {
        self.open.iter().find(|o| o.key == key)
    }

    /// Opens a block at the next free index after `start`, and returns that index.
    fn open_block(
        &mut self,
        key: Key,
        block: Block<'_>,
        out: &mut Vec<u8>,
    ) -> Result<u32, Failure> {
        self.start(out)?;
        let index = self.next;
        emit(
            out,
            &Out::ContentBlockStart {
                index,
                content_block: block,
            },
        )?;
        self.open.push(Open {
            key,
            index,
            had_delta: false,
        });
        self.next = index.saturating_add(1);
        Ok(index)
    }

    /// The index of the text block for this part, opened if it is not open.
    fn open_text(&mut self, output: u32, content: u32, out: &mut Vec<u8>) -> Result<u32, Failure> {
        let key = Key::Text { output, content };
        if let Some(open) = self.find(key) {
            return Ok(open.index);
        }
        self.open_block(key, Block::Text { text: "" }, out)
    }

    /// U3's sample carries `call_id` and `name` when the call is added.
    fn open_call(&mut self, output: u32, item: &Item, out: &mut Vec<u8>) -> Result<(), Failure> {
        let id = item.call_id.as_deref().filter(|s| !s.is_empty());
        let name = item.name.as_deref().filter(|s| !s.is_empty());
        let (Some(id), Some(name)) = (id, name) else {
            return Err(Failure::Unreadable);
        };
        let block = Block::ToolUse {
            id,
            name,
            input: Empty {},
        };
        self.open_block(Key::Call { output }, block, out)?;
        self.called = true;
        Ok(())
    }

    /// `content_block_stop` for every open block `pick` selects, in index order.
    fn close_where(
        &mut self,
        out: &mut Vec<u8>,
        pick: impl Fn(Key) -> bool,
    ) -> Result<(), Failure> {
        let (closing, kept) = std::mem::take(&mut self.open)
            .into_iter()
            .partition::<Vec<_>, _>(|o| pick(o.key));
        self.open = kept;
        closing
            .iter()
            .try_for_each(|o| emit(out, &Out::ContentBlockStop { index: o.index }))
    }

    /// The one finish step: Start, close every open block, then `message_delta` with the stop
    /// reason and the usage, then `message_stop`.
    fn finish(
        &mut self,
        stop_reason: &'static str,
        usage: Option<Object<Usage>>,
        out: &mut Vec<u8>,
    ) -> Result<(), Failure> {
        self.start(out)?;
        self.close_where(out, |_| true)?;
        let delta = Stop {
            stop_reason,
            stop_sequence: None,
        };
        let usage = Used::from(usage.map(|Object(u)| u));
        emit(out, &Out::MessageDelta { delta, usage })?;
        emit(out, &Out::MessageStop)
    }
}

/// F17: the incomplete reasons this route has a stop reason for (U2, U7, U10).
fn incomplete_stop(reason: &str) -> Option<&'static str> {
    match reason {
        "max_output_tokens" => Some("max_tokens"),
        "content_filter" => Some("refusal"),
        _ => None,
    }
}

/// `T` from an event's data, a JSON object only. Any serde error is `Unreadable`.
fn read<'a, T: Deserialize<'a>>(data: &'a [u8]) -> Result<T, Failure> {
    serde_json::from_slice::<Object<T>>(data)
        .map(|Object(value)| value)
        .map_err(|_| Failure::Unreadable)
}

/// Appends one Anthropic event, framed under its own type (U11).
fn emit(out: &mut Vec<u8>, event: &Out<'_>) -> Result<(), Failure> {
    let data = serde_json::to_vec(event).map_err(|_| Failure::Unreadable)?;
    out.extend_from_slice(&sse::frame(event.name(), &data));
    Ok(())
}

#[cfg(test)]
mod tests;
