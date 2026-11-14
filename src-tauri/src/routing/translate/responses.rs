//! Building the Responses request (U1, "Create a model response"). Pure. Typed `Serialize`
//! structs only: every member written here is one U1 documents, and no other member is ever
//! written. Never written: `instructions`, `include`, `previous_response_id`, `conversation`,
//! `metadata`, `user`, `safety_identifier`, `prompt_cache_key`, `temperature`, `top_p`,
//! `truncation`, `text`, `service_tier`, `background`, an item `id`, an item `status`, a message
//! `phase` (F7, F8). Nothing here knows a model name: the model and the effort are the ones
//! A Human mapped for the connection and tier.

use super::Carry;
use super::messages::{Asked, Choice, Image, Piece, Returned, Turn};
use serde::Serialize;
use serde_json::value::RawValue;
use std::borrow::Cow;

/// The first line of a failed tool call's output (F11).
pub const ERROR_MARK: &str = "[tool call failed]";
/// U1: `max_output_tokens` has `minimum: 16`.
pub const MIN_OUTPUT_TOKENS: u64 = 16;

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    input: Vec<Item<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Function<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<ToolChoice<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<Reasoning<'a>>,
    max_output_tokens: u64,
    store: bool,
    stream: bool,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Item<'a> {
    Message {
        role: Role,
        content: Content<'a>,
    },
    /// No `id` and no `status` (F8).
    FunctionCall {
        call_id: &'a str,
        name: &'a str,
        arguments: &'a str,
    },
    FunctionCallOutput {
        call_id: &'a str,
        output: Content<'a>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    User,
    Assistant,
    Developer,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Content<'a> {
    Text(Cow<'a, str>),
    Parts(Vec<Part<'a>>),
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Part<'a> {
    InputText {
        text: Cow<'a, str>,
    },
    InputImage {
        image_url: Cow<'a, str>,
        detail: &'static str,
    },
}

#[derive(Serialize)]
struct Function<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    parameters: &'a RawValue,
    /// Always false (F9, U3: "explicitly set `strict: false`").
    strict: bool,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ToolChoice<'a> {
    Mode(&'static str),
    Function {
        #[serde(rename = "type")]
        kind: &'static str,
        name: &'a str,
    },
}

#[derive(Serialize)]
struct Reasoning<'a> {
    effort: &'a str,
}

/// The Responses request for `asked`, as the bytes to post. `model` and `effort` are the ones
/// A Human mapped for the connection and tier: nothing here knows a model name.
/// Err: only a piece in a place the reader never puts one, or a serialisation failure, neither
/// of which can happen. Both are `Carry::Unknown`, so nothing is sent.
pub fn build(asked: &Asked<'_>, model: &str, effort: Option<&str>) -> Result<Vec<u8>, Carry> {
    let mut input = Vec::new();
    if !asked.system.is_empty() {
        input.push(developer(&asked.system));
    }
    for turn in &asked.turns {
        match turn {
            Turn::System(texts) => input.push(developer(texts)),
            Turn::User(pieces) => user(pieces, &mut input)?,
            Turn::Assistant(pieces) => assistant(pieces, &mut input)?,
        }
    }
    let request = Request {
        model,
        input,
        tools: asked
            .tools
            .iter()
            .map(|tool| Function {
                kind: "function",
                name: &tool.name,
                description: tool.description.as_deref(),
                parameters: tool.schema,
                strict: false,
            })
            .collect(),
        // U1, U3: no choice leaves OpenAI's default, `auto`.
        tool_choice: asked.choice.as_ref().map(|choice| match choice {
            Choice::Auto => ToolChoice::Mode("auto"),
            Choice::Any => ToolChoice::Mode("required"),
            Choice::None => ToolChoice::Mode("none"),
            Choice::Tool(name) => ToolChoice::Function {
                kind: "function",
                name,
            },
        }),
        // U3: "You can prevent this by setting `parallel_tool_calls` to `false`".
        parallel_tool_calls: (!asked.parallel).then_some(false),
        reasoning: effort.map(|effort| Reasoning { effort }),
        max_output_tokens: asked.max_tokens.max(MIN_OUTPUT_TOKENS),
        store: false,
        stream: true,
    };
    serde_json::to_vec(&request).map_err(|_| Carry::Unknown)
}

/// System text as a `developer` message, one part per text (F7).
fn developer<'a>(texts: &'a [Cow<'a, str>]) -> Item<'a> {
    Item::Message {
        role: Role::Developer,
        content: Content::Parts(texts.iter().map(|text| input_text(text)).collect()),
    }
}

/// A user turn in order: each run of text and images is one message, each result one output.
fn user<'a>(pieces: &'a [Piece<'a>], input: &mut Vec<Item<'a>>) -> Result<(), Carry> {
    let mut run = Vec::new();
    for piece in pieces {
        if let Piece::Result { id, body, failed } = piece {
            flush(&mut run, input);
            input.push(Item::FunctionCallOutput {
                call_id: id,
                output: output(body, *failed)?,
            });
        } else {
            run.push(part(piece)?);
        }
    }
    flush(&mut run, input);
    Ok(())
}

fn flush<'a>(run: &mut Vec<Part<'a>>, input: &mut Vec<Item<'a>>) {
    if !run.is_empty() {
        input.push(Item::Message {
            role: Role::User,
            content: Content::Parts(std::mem::take(run)),
        });
    }
}

/// An assistant turn in order: each text one message, each call one `function_call`.
fn assistant<'a>(pieces: &'a [Piece<'a>], input: &mut Vec<Item<'a>>) -> Result<(), Carry> {
    for piece in pieces {
        input.push(match piece {
            Piece::Text(text) => Item::Message {
                role: Role::Assistant,
                content: Content::Text(Cow::Borrowed(text)),
            },
            // F8: the input's own JSON text, as a string.
            Piece::Call { id, name, input } => Item::FunctionCall {
                call_id: id,
                name,
                arguments: input.get(),
            },
            Piece::Image(_) | Piece::Result { .. } => return Err(Carry::Unknown),
        });
    }
    Ok(())
}

/// What a tool call came to. A failed one starts with `ERROR_MARK` (F11).
fn output<'a>(body: &'a Returned<'a>, failed: bool) -> Result<Content<'a>, Carry> {
    Ok(match body {
        Returned::Text(text) if failed => {
            Content::Text(Cow::Owned(format!("{ERROR_MARK}\n{text}")))
        }
        Returned::Text(text) => Content::Text(Cow::Borrowed(text)),
        Returned::Parts(pieces) => {
            let mark = failed.then(|| input_text(ERROR_MARK));
            Content::Parts(
                mark.into_iter()
                    .map(Ok)
                    .chain(pieces.iter().map(part))
                    .collect::<Result<_, _>>()?,
            )
        }
    })
}

/// A text or an image as an input part. Images go as a web address or a data URL (U1, U8).
fn part<'a>(piece: &'a Piece<'a>) -> Result<Part<'a>, Carry> {
    Ok(match piece {
        Piece::Text(text) => input_text(text),
        Piece::Image(Image::Base64 { media_type, data }) => Part::InputImage {
            image_url: Cow::Owned(format!("data:{media_type};base64,{data}")),
            detail: "auto",
        },
        Piece::Image(Image::Url(url)) => Part::InputImage {
            image_url: Cow::Borrowed(url),
            detail: "auto",
        },
        Piece::Call { .. } | Piece::Result { .. } => return Err(Carry::Unknown),
    })
}

fn input_text(text: &str) -> Part<'_> {
    Part::InputText {
        text: Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests;
