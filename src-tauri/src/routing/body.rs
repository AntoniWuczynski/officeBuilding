//! Reading what a Messages request asks for, and editing a body before it goes to one
//! connection. Pure. No `serde_json::Value`: typed probes read the few members the router
//! needs, and `RawValue` carries every other member as the bytes it arrived as.

use super::capabilities::Capability;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::value::{RawValue, to_raw_value};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;
use std::marker::PhantomData;

/// What the router reads from a Messages request. No content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub model: String,
    /// The top-level `output_config.effort` (E18).
    pub effort: Option<String>,
    pub capabilities: BTreeSet<Capability>,
}

/// The body is not a JSON object with a text `model`. No detail: a serde message can quote the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

/// What the router changes in a body before it goes to one connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edit<'a> {
    /// Replace the top-level `model`.
    pub model: Option<&'a str>,
    /// Replace the top-level `output_config.effort`. Ignored when the body has none.
    pub effort: Option<&'a str>,
    /// Remove every `thinking` and `redacted_thinking` block from `messages`.
    pub drop_thinking: bool,
    /// Set `provider.allow_fallbacks` to false and `provider.require_parameters` to true.
    pub pin_provider: bool,
}

/// A scan that cannot follow the body reports every capability (E24).
const ALL: [Capability; 4] = [
    Capability::Tools,
    Capability::Images,
    Capability::Documents,
    Capability::ServerTools,
];

/// The block types that carry hidden reasoning (T12: remove "every `thinking` and
/// `redacted_thinking` block [...] Keep every other block").
const THINKING: [&str; 2] = ["thinking", "redacted_thinking"];

/// What a request asks for. A repeated `model`, `output_config`, `messages` or `provider` is
/// refused, so the router never judges one value while the upstream reads another.
pub fn read(body: &[u8]) -> Result<Facts, Unreadable> {
    let Object(head) = serde_json::from_slice::<Object<Head<'_>>>(body).map_err(|_| Unreadable)?;
    Ok(Facts {
        model: head.model.into_owned(),
        effort: head
            .output_config
            .and_then(|Object(config)| config.effort)
            .map(Cow::into_owned),
        capabilities: scan(body),
    })
}

/// The body with `edit` applied. Borrowed, the very bytes that came in, when nothing changed.
/// Every member the router does not own keeps the bytes it arrived as, at every depth.
pub fn rewrite<'a>(body: &'a [u8], edit: &Edit<'_>) -> Result<Cow<'a, [u8]>, Unreadable> {
    if *edit == Edit::default() {
        return Ok(Cow::Borrowed(body));
    }
    let mut top: Members<'_> = serde_json::from_slice(body).map_err(|_| Unreadable)?;
    let mut changed = false;
    if let Some(model) = edit.model {
        changed |= set_model(&mut top, model)?;
    }
    if let Some(effort) = edit.effort {
        changed |= set_effort(&mut top, effort)?;
    }
    if edit.drop_thinking {
        changed |= drop_thinking(&mut top)?;
    }
    if edit.pin_provider {
        changed |= pin_provider(&mut top)?;
    }
    if !changed {
        return Ok(Cow::Borrowed(body));
    }
    serde_json::to_vec(&top)
        .map(Cow::Owned)
        .map_err(|_| Unreadable)
}

/// `T` read from a JSON object only. serde's derived structs also accept an array, by
/// position, which would let `["claude-opus-5-5"]` pass as a body with a model.
pub(crate) struct Object<T>(pub T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for OnlyMap<T> {
            type Value = T;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
                T::deserialize(MapAccessDeserializer::new(map))
            }
        }
        deserializer
            .deserialize_map(OnlyMap(PhantomData))
            .map(Object)
    }
}

/// The first pass: what selection needs, and the members a later edit may touch, held raw so
/// that a repeat of any of them is refused here.
#[derive(Deserialize)]
struct Head<'a> {
    #[serde(borrow)]
    model: Cow<'a, str>,
    #[serde(default, borrow)]
    output_config: Option<Object<OutputConfig<'a>>>,
    #[serde(default, borrow, rename = "messages")]
    _messages: Option<&'a RawValue>,
    #[serde(default, borrow, rename = "provider")]
    _provider: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct OutputConfig<'a> {
    #[serde(default, borrow)]
    effort: Option<Cow<'a, str>>,
}

/// The second pass: advice only. Any failure reports every capability.
#[derive(Deserialize)]
struct Scan<'a> {
    #[serde(default, borrow)]
    tools: Option<Vec<Tool<'a>>>,
    #[serde(default)]
    messages: Option<Vec<Turn>>,
}

#[derive(Deserialize)]
struct Tool<'a> {
    #[serde(default, borrow, rename = "type")]
    kind: Option<Cow<'a, str>>,
}

#[derive(Deserialize)]
struct Turn {
    #[serde(default)]
    content: Option<Content>,
}

/// One content block. A tool result's own `content` is scanned too.
#[derive(Deserialize)]
struct Block<'a> {
    #[serde(borrow, rename = "type")]
    kind: Cow<'a, str>,
    #[serde(default)]
    content: Option<Content>,
}

/// The capabilities found in one `content`: a string is text, an array is blocks. Read by
/// hand so the conversation's strings are looked at and never copied.
struct Content(BTreeSet<Capability>);

impl<'de> Deserialize<'de> for Content {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Blocks;
        impl<'de> Visitor<'de> for Blocks {
            type Value = Content;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string or an array of blocks")
            }
            fn visit_str<E: de::Error>(self, _text: &str) -> Result<Content, E> {
                Ok(Content(BTreeSet::new()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Content, A::Error> {
                let mut found = BTreeSet::new();
                while let Some(block) = seq.next_element::<Block<'de>>()? {
                    match block.kind.as_ref() {
                        "image" => {
                            found.insert(Capability::Images);
                        }
                        "document" => {
                            found.insert(Capability::Documents);
                        }
                        _ => {}
                    }
                    if let Some(Content(inner)) = block.content {
                        found.extend(inner);
                    }
                }
                Ok(Content(found))
            }
        }
        deserializer.deserialize_any(Blocks)
    }
}

fn scan(body: &[u8]) -> BTreeSet<Capability> {
    let Ok(scan) = serde_json::from_slice::<Scan<'_>>(body) else {
        return ALL.into_iter().collect();
    };
    let mut found = BTreeSet::new();
    for tool in scan.tools.into_iter().flatten() {
        found.insert(match tool.kind.as_deref() {
            None | Some("custom") => Capability::Tools,
            Some(_) => Capability::ServerTools,
        });
    }
    for turn in scan.messages.into_iter().flatten() {
        if let Some(Content(inner)) = turn.content {
            found.extend(inner);
        }
    }
    found
}

/// A JSON object's members in the order they arrived. A value is the bytes it arrived as,
/// or bytes the router wrote. A repeated key is refused.
struct Members<'a>(Vec<(String, Cow<'a, RawValue>)>);

impl<'de: 'a, 'a> Deserialize<'de> for Members<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries<'a>(PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for Entries<'a> {
            type Value = Members<'a>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members<'a>, A::Error> {
                let mut members: Vec<(String, Cow<'a, RawValue>)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, &'a RawValue>()? {
                    if members.iter().any(|(seen, _)| *seen == key) {
                        return Err(de::Error::custom("a repeated member"));
                    }
                    members.push((key, Cow::Borrowed(value)));
                }
                Ok(Members(members))
            }
        }
        deserializer.deserialize_map(Entries(PhantomData))
    }
}

impl Serialize for Members<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value.as_ref())?;
        }
        map.end()
    }
}

impl<'a> Members<'a> {
    fn parse(raw: &'a RawValue) -> Result<Self, Unreadable> {
        serde_json::from_str(raw.get()).map_err(|_| Unreadable)
    }

    fn get(&self, key: &str) -> Option<&RawValue> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_ref())
    }

    /// Replace the member in place, or append it.
    fn set(&mut self, key: &str, value: Box<RawValue>) {
        match self.0.iter_mut().find(|(name, _)| name == key) {
            Some((_, slot)) => *slot = Cow::Owned(value),
            None => self.0.push((key.to_owned(), Cow::Owned(value))),
        }
    }

    /// Set `key` to `value`. Whether that changed anything.
    fn pin(&mut self, key: &str, value: bool) -> Result<bool, Unreadable> {
        let current = self
            .get(key)
            .and_then(|raw| serde_json::from_str::<bool>(raw.get()).ok());
        if current == Some(value) {
            return Ok(false);
        }
        self.set(key, raw(&value)?);
        Ok(true)
    }
}

fn raw<T: Serialize + ?Sized>(value: &T) -> Result<Box<RawValue>, Unreadable> {
    to_raw_value(value).map_err(|_| Unreadable)
}

fn text(raw: &RawValue) -> Result<String, Unreadable> {
    serde_json::from_str(raw.get()).map_err(|_| Unreadable)
}

fn set_model(top: &mut Members<'_>, model: &str) -> Result<bool, Unreadable> {
    if text(top.get("model").ok_or(Unreadable)?)? == model {
        return Ok(false);
    }
    top.set("model", raw(model)?);
    Ok(true)
}

fn set_effort(top: &mut Members<'_>, effort: &str) -> Result<bool, Unreadable> {
    let Some(config) = top.get("output_config") else {
        return Ok(false);
    };
    let Some(mut config) =
        serde_json::from_str::<Option<Members<'_>>>(config.get()).map_err(|_| Unreadable)?
    else {
        return Ok(false);
    };
    let Some(current) = config.get("effort") else {
        return Ok(false);
    };
    if text(current)? == effort {
        return Ok(false);
    }
    config.set("effort", raw(effort)?);
    let value = raw(&config)?;
    top.set("output_config", value);
    Ok(true)
}

/// What is left of one message once its hidden reasoning is out.
enum Kept {
    Whole,
    Edited(Box<RawValue>),
    Nothing,
}

/// Never fails on a shape it does not expect: anything it cannot follow is left as it is.
fn drop_thinking(top: &mut Members<'_>) -> Result<bool, Unreadable> {
    let Some(messages) = top.get("messages") else {
        return Ok(false);
    };
    let Ok(messages) = serde_json::from_str::<Vec<&RawValue>>(messages.get()) else {
        return Ok(false);
    };
    let mut changed = false;
    let mut kept: Vec<Cow<'_, RawValue>> = Vec::with_capacity(messages.len());
    for message in messages {
        match without_thinking(message)? {
            Kept::Whole => kept.push(Cow::Borrowed(message)),
            Kept::Edited(value) => {
                changed = true;
                kept.push(Cow::Owned(value));
            }
            // Consecutive turns of one role are combined (T18), so a turn may go.
            Kept::Nothing => changed = true,
        }
    }
    if !changed {
        return Ok(false);
    }
    let value = raw(&kept)?;
    top.set("messages", value);
    Ok(true)
}

fn without_thinking(message: &RawValue) -> Result<Kept, Unreadable> {
    let Ok(mut members) = serde_json::from_str::<Members<'_>>(message.get()) else {
        return Ok(Kept::Whole);
    };
    let Some(content) = members.get("content") else {
        return Ok(Kept::Whole);
    };
    let Ok(blocks) = serde_json::from_str::<Vec<&RawValue>>(content.get()) else {
        return Ok(Kept::Whole);
    };
    let total = blocks.len();
    let blocks: Vec<&RawValue> = blocks
        .into_iter()
        .filter(|block| !is_thinking(block))
        .collect();
    if blocks.len() == total {
        return Ok(Kept::Whole);
    }
    if blocks.is_empty() {
        return Ok(Kept::Nothing);
    }
    let value = raw(&blocks)?;
    members.set("content", value);
    Ok(Kept::Edited(raw(&members)?))
}

#[derive(Deserialize)]
struct Typed<'a> {
    #[serde(borrow, rename = "type")]
    kind: Cow<'a, str>,
}

fn is_thinking(block: &RawValue) -> bool {
    serde_json::from_str::<Object<Typed<'_>>>(block.get())
        .is_ok_and(|Object(block)| THINKING.contains(&block.kind.as_ref()))
}

/// E10: the fallback of `OpenRouter` itself off, and no silent drop of a parameter (T15).
fn pin_provider(top: &mut Members<'_>) -> Result<bool, Unreadable> {
    let mut provider = match top.get("provider") {
        Some(current) if current.get().starts_with('{') => Members::parse(current)?,
        // Absent, or not an object: replaced whole.
        _ => Members(Vec::new()),
    };
    let mut changed = provider.pin("allow_fallbacks", false)?;
    changed |= provider.pin("require_parameters", true)?;
    if !changed {
        return Ok(false);
    }
    let value = raw(&provider)?;
    top.set("provider", value);
    Ok(true)
}

#[cfg(test)]
mod tests;
