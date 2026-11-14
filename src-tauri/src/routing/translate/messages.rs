//! Reading a Messages request strictly (F4). Pure. No `serde_json::Value`: each member is read
//! from its `RawValue` into a typed struct, and a polymorphic member is told apart by its first
//! byte, never by an untagged or internally tagged enum (both buffer their input).
//!
//! Every member the reader meets is carried, left out on purpose (F5: `cache_control`,
//! `metadata`, `thinking`, a tool's `eager_input_streaming`, and hidden reasoning blocks), or it
//! refuses the request. A serde error is never formatted: it can quote the input.

use super::{Carry, label};
use crate::routing::body::{Members, Object, Typed};
use crate::routing::config::effort_name;
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::value::RawValue;
use std::borrow::Cow;

/// What a Messages request asks for, as far as this route carries it. Borrowed from the body.
#[cfg_attr(test, derive(Debug))]
pub struct Asked<'a> {
    /// The top-level `system`, one entry per text block (one for a string). Empty: none.
    pub system: Vec<Cow<'a, str>>,
    pub turns: Vec<Turn<'a>>,
    pub tools: Vec<Tool<'a>>,
    pub choice: Option<Choice<'a>>,
    /// False: `tool_choice.disable_parallel_tool_use` was true.
    pub parallel: bool,
    /// `max_tokens`, never 0.
    pub max_tokens: u64,
    /// The effort in force for this reply (U12), as the desk named it. None: the request names
    /// none.
    pub effort: Option<String>,
}

#[cfg_attr(test, derive(Debug))]
pub enum Turn<'a> {
    User(Vec<Piece<'a>>),
    Assistant(Vec<Piece<'a>>),
    /// A `role: "system"` entry of `messages` that holds text.
    System(Vec<Cow<'a, str>>),
}

#[cfg_attr(test, derive(Debug))]
pub enum Piece<'a> {
    Text(Cow<'a, str>),
    Image(Image<'a>),
    /// A tool call the model made: its id, the tool's name, and its input as the JSON text it
    /// arrived as.
    Call {
        id: Cow<'a, str>,
        name: Cow<'a, str>,
        input: &'a RawValue,
    },
    /// What a tool call came to.
    Result {
        id: Cow<'a, str>,
        body: Returned<'a>,
        failed: bool,
    },
}

#[cfg_attr(test, derive(Debug))]
pub enum Returned<'a> {
    /// `content` was a string, or absent (the empty string).
    Text(Cow<'a, str>),
    /// `content` was a list: `Piece::Text` and `Piece::Image` only.
    Parts(Vec<Piece<'a>>),
}

#[cfg_attr(test, derive(Debug))]
pub enum Image<'a> {
    Base64 {
        media_type: Cow<'a, str>,
        data: Cow<'a, str>,
    },
    Url(Cow<'a, str>),
}

#[cfg_attr(test, derive(Debug))]
pub struct Tool<'a> {
    pub name: Cow<'a, str>,
    pub description: Option<Cow<'a, str>>,
    pub schema: &'a RawValue,
}

#[cfg_attr(test, derive(Debug))]
pub enum Choice<'a> {
    Auto,
    Any,
    None,
    Tool(Cow<'a, str>),
}

/// The image types both vendors document (U8, U10).
const MEDIA_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];

/// The longest tool call id `OpenAI` takes (U1: `call_id` `maxLength: 64`).
const CALL_ID_MAX: usize = 64;

/// Read `body`. Err: this route cannot carry it, and nothing may be sent. The members are
/// checked in body order and the first refusal is returned, so the sentence names something the
/// client can act on.
pub fn read(body: &[u8]) -> Result<Asked<'_>, Carry> {
    let top: Members<'_> = serde_json::from_slice(body).map_err(|_| Carry::Unknown)?;
    let mut asked = Asked {
        system: Vec::new(),
        turns: Vec::new(),
        tools: Vec::new(),
        choice: None,
        parallel: true,
        max_tokens: 0,
        effort: None,
    };
    let mut steps = Vec::new();
    let mut top_effort = None;
    let mut streamed = false;
    for (name, value) in top.iter() {
        // A member read from a body is always borrowed (`Members::iter`).
        let &Cow::Borrowed(value) = value else {
            return Err(Carry::Unknown);
        };
        match name {
            // `model` is not read here: the lease's target model is what is sent. The rest are
            // left out on purpose (F5), their values never read.
            "model" | "metadata" | "thinking" | "cache_control" => {}
            "messages" => {
                let (turns, walk) = conversation(value)?;
                asked.turns = turns;
                steps = walk;
            }
            "system" => {
                asked.system = match parse::<Option<&RawValue>>(value)? {
                    Some(content) => texts(content)?,
                    None => Vec::new(),
                };
            }
            "tools" => asked.tools = tools(value)?,
            "tool_choice" => {
                (asked.choice, asked.parallel) = match parse::<Option<&RawValue>>(value)? {
                    Some(raw) => choice(raw)?,
                    None => (None, true),
                };
            }
            "max_tokens" => {
                asked.max_tokens = parse::<u64>(value)?;
                if asked.max_tokens == 0 {
                    return Err(Carry::Prewarm);
                }
            }
            "stream" => {
                streamed = parse::<Option<bool>>(value)?.unwrap_or(false);
                if !streamed {
                    return Err(Carry::NotStreamed);
                }
            }
            "output_config" => top_effort = effort(value)?,
            "stop_sequences" => {
                if !parse::<Option<Vec<IgnoredAny>>>(value)?
                    .unwrap_or_default()
                    .is_empty()
                {
                    return Err(Carry::StopSequences);
                }
            }
            "context_management" => {
                if parse::<Option<IgnoredAny>>(value)?.is_some() {
                    return Err(Carry::ContextManagement);
                }
            }
            other => {
                return Err(label(other).map_or(Carry::Unknown, |name| Carry::Member { name }));
            }
        }
    }
    if !streamed {
        return Err(Carry::NotStreamed);
    }
    if asked.max_tokens == 0 {
        return Err(Carry::Unknown);
    }
    asked.effort = in_force(top_effort, steps);
    Ok(asked)
}

/// One turn as the effort walk (U12) sees it.
enum Step {
    /// A `user` turn, and whether it held new input.
    User {
        fresh: bool,
    },
    Assistant,
    /// A `system` entry, and the effort it names.
    System(Option<String>),
}

/// The effort in force for the reply to the last turn (U12): an entry directly after a `user`
/// turn with new input applies at once, anywhere else from the next `user` turn with new input.
/// A `user` turn of only tool results is not new input.
fn in_force(top: Option<String>, steps: Vec<Step>) -> Option<String> {
    let mut level = top;
    let mut pending = None;
    let mut fresh = false;
    for step in steps {
        match step {
            Step::System(Some(effort)) if fresh => level = Some(effort),
            Step::System(Some(effort)) => pending = Some(effort),
            Step::System(None) => {}
            Step::User { fresh: new_input } => {
                if new_input && let Some(effort) = pending.take() {
                    level = Some(effort);
                }
                fresh = new_input;
            }
            Step::Assistant => fresh = false,
        }
    }
    level
}

/// `T` from one member's raw text. Any serde error is `Unknown`.
fn parse<'a, T: Deserialize<'a>>(raw: &'a RawValue) -> Result<T, Carry> {
    serde_json::from_str(raw.get()).map_err(|_| Carry::Unknown)
}

/// `T` from a JSON object only (`body::Object`: a derived struct also takes an array by position).
fn object<'a, T: Deserialize<'a>>(raw: &'a RawValue) -> Result<T, Carry> {
    parse::<Object<T>>(raw).map(|Object(value)| value)
}

/// A string that borrows from the body when it has no escapes.
#[derive(Deserialize)]
struct Str<'a>(#[serde(borrow)] Cow<'a, str>);

/// A `content`: a string or a list, told apart by its first byte.
enum Shape<'a> {
    Text(Cow<'a, str>),
    List(Vec<&'a RawValue>),
}

fn shape(raw: &RawValue) -> Result<Shape<'_>, Carry> {
    match raw.get().as_bytes().first() {
        Some(b'"') => parse::<Str<'_>>(raw).map(|Str(text)| Shape::Text(text)),
        Some(b'[') => parse(raw).map(Shape::List),
        _ => Err(Carry::Unknown),
    }
}

/// A block type this route does not carry, named when the name is fit for a sentence.
fn refused_block(kind: &str) -> Carry {
    label(kind).map_or(Carry::Unknown, |kind| Carry::Block { kind })
}

fn kind_of(block: &RawValue) -> Result<Cow<'_, str>, Carry> {
    object::<Typed<'_>>(block).map(|typed| typed.kind)
}

/// An `output_config`: `effort` only, a string or null. Another member is `OutputSetting`.
fn effort(raw: &RawValue) -> Result<Option<String>, Carry> {
    let Some(config) = parse::<Option<Members<'_>>>(raw)? else {
        return Ok(None);
    };
    let mut found = None;
    for (name, value) in config.iter() {
        if name != "effort" {
            return Err(Carry::OutputSetting);
        }
        found = match parse::<Option<Str<'_>>>(value)? {
            Some(Str(name)) => Some(effort_name(&name).ok_or(Carry::Unknown)?),
            None => None,
        };
    }
    Ok(found)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTurn<'a> {
    #[serde(borrow)]
    role: Cow<'a, str>,
    #[serde(borrow, default)]
    content: Option<&'a RawValue>,
    #[serde(borrow, default)]
    output_config: Option<&'a RawValue>,
}

/// `messages`: the turns this route carries, and every turn as the effort walk sees it.
fn conversation(raw: &RawValue) -> Result<(Vec<Turn<'_>>, Vec<Step>), Carry> {
    let mut turns = Vec::new();
    let mut steps = Vec::new();
    for entry in parse::<Vec<&RawValue>>(raw)? {
        let RawTurn {
            role,
            content,
            output_config,
        } = object(entry)?;
        let content = content.ok_or(Carry::Unknown)?;
        let (turn, step) = match role.as_ref() {
            "system" => {
                let effort = match output_config {
                    Some(config) => effort(config)?,
                    None => None,
                };
                let texts = texts(content)?;
                (
                    (!texts.is_empty()).then_some(Turn::System(texts)),
                    Step::System(effort),
                )
            }
            "user" if output_config.is_none() => {
                let pieces = pieces(content, Place::User)?;
                let fresh = pieces
                    .iter()
                    .any(|piece| !matches!(piece, Piece::Result { .. }));
                (Some(Turn::User(pieces)), Step::User { fresh })
            }
            "assistant" if output_config.is_none() => (
                Some(Turn::Assistant(pieces(content, Place::Assistant)?)),
                Step::Assistant,
            ),
            _ => return Err(Carry::Unknown),
        };
        steps.push(step);
        // A turn left with no piece (an assistant turn that held only thinking) is dropped.
        match turn {
            Some(Turn::User(pieces) | Turn::Assistant(pieces)) if pieces.is_empty() => {}
            Some(turn) => turns.push(turn),
            None => {}
        }
    }
    Ok((turns, steps))
}

/// Where a list of blocks sits, which decides the block types it may hold.
#[derive(Clone, Copy)]
enum Place {
    User,
    Assistant,
    /// A tool result's own `content`.
    Returned,
}

fn pieces(content: &RawValue, place: Place) -> Result<Vec<Piece<'_>>, Carry> {
    match shape(content)? {
        Shape::Text(text) => Ok(vec![Piece::Text(text)]),
        Shape::List(blocks) => {
            let mut pieces = Vec::with_capacity(blocks.len());
            for block in blocks {
                if let Some(piece) = piece(block, place)? {
                    pieces.push(piece);
                }
            }
            Ok(pieces)
        }
    }
}

/// One block. None: hidden reasoning, left out here and always (A Human's ruling).
fn piece(block: &RawValue, place: Place) -> Result<Option<Piece<'_>>, Carry> {
    let kind = kind_of(block)?;
    let piece = match (place, kind.as_ref()) {
        (_, "text") => Piece::Text(text(block)?),
        (Place::User | Place::Returned, "image") => Piece::Image(image(block)?),
        (Place::User, "tool_result") => result(block)?,
        (Place::Assistant, "tool_use") => call(block)?,
        (Place::Assistant, "thinking" | "redacted_thinking") => return Ok(None),
        (_, other) => return Err(refused_block(other)),
    };
    Ok(Some(piece))
}

/// A `system` (top-level or an entry): a string, or a list of text blocks.
fn texts(content: &RawValue) -> Result<Vec<Cow<'_, str>>, Carry> {
    match shape(content)? {
        Shape::Text(text) => Ok(vec![text]),
        Shape::List(blocks) => blocks
            .into_iter()
            .map(|block| match kind_of(block)?.as_ref() {
                "text" => text(block),
                other => Err(refused_block(other)),
            })
            .collect(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextBlock<'a> {
    #[serde(default, rename = "type")]
    _kind: Option<IgnoredAny>,
    #[serde(borrow)]
    text: Cow<'a, str>,
    #[serde(default, rename = "cache_control")]
    _cache_control: Option<IgnoredAny>,
    #[serde(default)]
    citations: Option<Vec<IgnoredAny>>,
}

fn text(block: &RawValue) -> Result<Cow<'_, str>, Carry> {
    let TextBlock {
        text, citations, ..
    } = object(block)?;
    if citations.is_some_and(|list| !list.is_empty()) {
        return Err(refused_block("citations"));
    }
    Ok(text)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageBlock<'a> {
    #[serde(default, rename = "type")]
    _kind: Option<IgnoredAny>,
    #[serde(borrow)]
    source: &'a RawValue,
    #[serde(default, rename = "cache_control")]
    _cache_control: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source<'a> {
    #[serde(borrow, rename = "type")]
    kind: Cow<'a, str>,
    #[serde(borrow, default)]
    media_type: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    data: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    url: Option<Cow<'a, str>>,
}

fn image(block: &RawValue) -> Result<Image<'_>, Carry> {
    let ImageBlock { source, .. } = object(block)?;
    // A source the reader cannot follow, a file id included, is one this route cannot send.
    let source: Source<'_> = object(source).map_err(|_| Carry::ImageSource)?;
    match (
        source.kind.as_ref(),
        source.media_type,
        source.data,
        source.url,
    ) {
        ("base64", Some(media_type), Some(data), None)
            if MEDIA_TYPES.contains(&media_type.as_ref()) =>
        {
            Ok(Image::Base64 { media_type, data })
        }
        ("url", None, None, Some(url)) => Ok(Image::Url(url)),
        _ => Err(Carry::ImageSource),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallBlock<'a> {
    #[serde(default, rename = "type")]
    _kind: Option<IgnoredAny>,
    #[serde(borrow)]
    id: Cow<'a, str>,
    #[serde(borrow)]
    name: Cow<'a, str>,
    #[serde(borrow)]
    input: &'a RawValue,
    #[serde(borrow, default)]
    caller: Option<&'a RawValue>,
    #[serde(default, rename = "cache_control")]
    _cache_control: Option<IgnoredAny>,
}

/// A `caller` that says the model made the call itself.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Caller<'a> {
    #[serde(borrow, rename = "type")]
    kind: Cow<'a, str>,
}

fn call(block: &RawValue) -> Result<Piece<'_>, Carry> {
    let CallBlock {
        id,
        name,
        input,
        caller,
        ..
    } = object(block)?;
    if let Some(caller) = caller {
        // A call made by a tool the provider ran has no counterpart on this route.
        let direct = object::<Caller<'_>>(caller).is_ok_and(|caller| caller.kind == "direct");
        if !direct {
            return Err(refused_block("tool_use"));
        }
    }
    call_id(&id)?;
    if !input.get().starts_with('{') {
        return Err(Carry::Unknown);
    }
    Ok(Piece::Call { id, name, input })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultBlock<'a> {
    #[serde(default, rename = "type")]
    _kind: Option<IgnoredAny>,
    #[serde(borrow)]
    tool_use_id: Cow<'a, str>,
    #[serde(default)]
    is_error: Option<bool>,
    #[serde(borrow, default)]
    content: Option<&'a RawValue>,
    #[serde(default, rename = "cache_control")]
    _cache_control: Option<IgnoredAny>,
}

fn result(block: &RawValue) -> Result<Piece<'_>, Carry> {
    let ResultBlock {
        tool_use_id,
        is_error,
        content,
        ..
    } = object(block)?;
    call_id(&tool_use_id)?;
    let body = match content {
        None => Returned::Text(Cow::Borrowed("")),
        Some(content) => match shape(content)? {
            Shape::Text(text) => Returned::Text(text),
            Shape::List(_) => Returned::Parts(pieces(content, Place::Returned)?),
        },
    };
    Ok(Piece::Result {
        id: tool_use_id,
        body,
        failed: is_error.unwrap_or(false),
    })
}

/// U1: a `call_id` is 1 to 64 characters.
fn call_id(id: &str) -> Result<(), Carry> {
    if (1..=CALL_ID_MAX).contains(&id.chars().count()) {
        Ok(())
    } else {
        Err(Carry::CallId)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTool<'a> {
    #[serde(borrow, default, rename = "type")]
    kind: Option<Cow<'a, str>>,
    #[serde(borrow)]
    name: Cow<'a, str>,
    #[serde(borrow, default)]
    description: Option<Cow<'a, str>>,
    #[serde(borrow)]
    input_schema: &'a RawValue,
    #[serde(default)]
    strict: Option<bool>,
    #[serde(default)]
    defer_loading: Option<bool>,
    #[serde(default, rename = "cache_control")]
    _cache_control: Option<IgnoredAny>,
    #[serde(default, rename = "eager_input_streaming")]
    _eager_input_streaming: Option<IgnoredAny>,
}

/// `tools`: plain function tools only. `null` or an empty list is none.
fn tools(raw: &RawValue) -> Result<Vec<Tool<'_>>, Carry> {
    let Some(list) = parse::<Option<Vec<&RawValue>>>(raw)? else {
        return Ok(Vec::new());
    };
    list.into_iter()
        .map(|entry| {
            let tool: RawTool<'_> = object(entry).map_err(|_| Carry::Tool)?;
            let plain = matches!(tool.kind.as_deref(), None | Some("custom"))
                && tool.strict != Some(true)
                && tool.defer_loading != Some(true);
            if !plain {
                return Err(Carry::Tool);
            }
            Ok(Tool {
                name: tool.name,
                description: tool.description,
                schema: tool.input_schema,
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChoice<'a> {
    #[serde(borrow, rename = "type")]
    kind: Cow<'a, str>,
    #[serde(borrow, default)]
    name: Option<Cow<'a, str>>,
    #[serde(default)]
    disable_parallel_tool_use: Option<bool>,
}

/// `tool_choice`, and whether parallel tool calls stay allowed.
fn choice(raw: &RawValue) -> Result<(Option<Choice<'_>>, bool), Carry> {
    let RawChoice {
        kind,
        name,
        disable_parallel_tool_use,
    } = object(raw).map_err(|_| Carry::ToolChoice)?;
    let choice = match (kind.as_ref(), name) {
        ("auto", None) => Choice::Auto,
        ("any", None) => Choice::Any,
        ("none", None) => Choice::None,
        ("tool", Some(name)) => Choice::Tool(name),
        _ => return Err(Carry::ToolChoice),
    };
    Ok((Some(choice), disable_parallel_tool_use != Some(true)))
}

#[cfg(test)]
mod tests;
