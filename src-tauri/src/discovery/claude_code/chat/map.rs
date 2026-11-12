//! Transcript lines to conversation entries (spec section 4.5). Every line is
//! untrusted and the format is internal to Claude Code, so each field is read
//! leniently: a value of the wrong shape reads as absent, and a line or block
//! the mapper does not know makes no entry. Nothing here returns an error.

use crate::structured::{Entry, FileDiff, ToolStatus, capped, difftext};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use std::collections::{HashMap, HashSet, VecDeque};

/// The most entries a conversation keeps: older ones are dropped.
pub(super) const KEPT: usize = 1000;
/// Lines longer than this are not parsed.
pub(super) const LINE_MAX: usize = 2 * 1024 * 1024;
/// The most characters of a prompt that did not come from A Human.
const NOTICE_MAX: usize = 500;

/// `T` as it reads from the value, or its default when the value has another shape.
/// The JSON value is only a stepping stone here and never leaves this function.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// A list item that is skipped, not fatal, when it has a shape the mapper does not know.
struct Loose<T>(Option<T>);

impl<'de, T: DeserializeOwned> Deserialize<'de> for Loose<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        lenient::<D, Option<T>>(d).map(Self)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    kind: String,
    #[serde(default, deserialize_with = "lenient")]
    is_sidechain: bool,
    #[serde(default, deserialize_with = "lenient")]
    is_meta: bool,
    #[serde(default, deserialize_with = "lenient")]
    is_api_error_message: bool,
    #[serde(default, deserialize_with = "lenient")]
    origin: Option<Origin>,
    #[serde(default, deserialize_with = "lenient")]
    message: Option<Message>,
}

#[derive(Deserialize, Default)]
struct Origin {
    #[serde(default, deserialize_with = "lenient")]
    kind: String,
}

#[derive(Deserialize, Default)]
struct Message {
    #[serde(default, deserialize_with = "lenient")]
    content: Content,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Loose<Block>>),
    #[default]
    Missing,
}

impl Content {
    /// The known blocks, with their index in the list. A string is one text block.
    fn blocks(self) -> Vec<(usize, Block)> {
        match self {
            Self::Text(text) => vec![(0, Block::Text { text })],
            Self::Blocks(blocks) => blocks
                .into_iter()
                .enumerate()
                .filter_map(|(i, b)| b.0.map(|b| (i, b)))
                .collect(),
            Self::Missing => Vec::new(),
        }
    }

    /// The text blocks joined by line breaks, with `[image]` for an image.
    fn text(self) -> String {
        self.blocks()
            .into_iter()
            .filter_map(|(_, b)| b.prose())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    Text {
        #[serde(default, deserialize_with = "lenient")]
        text: String,
    },
    Thinking {
        #[serde(default, deserialize_with = "lenient")]
        thinking: String,
    },
    ToolUse {
        #[serde(default, deserialize_with = "lenient")]
        id: String,
        #[serde(default, deserialize_with = "lenient")]
        name: String,
        #[serde(default, deserialize_with = "lenient")]
        input: Box<ToolInput>,
    },
    ToolResult {
        #[serde(default, deserialize_with = "lenient")]
        tool_use_id: String,
        #[serde(default, deserialize_with = "lenient")]
        content: Content,
        #[serde(default, deserialize_with = "lenient")]
        is_error: bool,
    },
    Image {},
    #[serde(other)]
    Other,
}

impl Block {
    /// What a text block or an image reads as in a prompt or a tool result.
    fn prose(&self) -> Option<String> {
        match self {
            Self::Text { text } => Some(text.clone()),
            Self::Image {} => Some("[image]".to_string()),
            _ => None,
        }
    }
}

/// The few inputs the chat shows. A field that is not a string reads as absent.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ToolInput {
    #[serde(deserialize_with = "lenient")]
    command: Option<String>,
    #[serde(deserialize_with = "lenient")]
    file_path: Option<String>,
    #[serde(deserialize_with = "lenient")]
    old_string: Option<String>,
    #[serde(deserialize_with = "lenient")]
    new_string: Option<String>,
    #[serde(deserialize_with = "lenient")]
    content: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pattern: Option<String>,
    #[serde(deserialize_with = "lenient")]
    url: Option<String>,
    #[serde(deserialize_with = "lenient")]
    query: Option<String>,
    #[serde(deserialize_with = "lenient")]
    description: Option<String>,
    #[serde(deserialize_with = "lenient")]
    prompt: Option<String>,
}

/// `text`'s lines joined by spaces, blank ones left out.
fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// How a tool call is named in its card.
fn title(name: &str, input: &ToolInput) -> String {
    let name = if name.is_empty() { "Tool call" } else { name };
    let detail = match name {
        "Bash" => {
            return input
                .command
                .as_deref()
                .map_or_else(|| name.to_string(), one_line);
        }
        "Edit" | "Write" | "Read" => input.file_path.as_ref(),
        _ => [
            &input.pattern,
            &input.url,
            &input.query,
            &input.description,
            &input.prompt,
        ]
        .into_iter()
        .find_map(Option::as_ref),
    };
    match detail {
        Some(detail) => one_line(&format!("{name} {detail}")),
        None => name.to_string(),
    }
}

/// The file change a tool call makes, as the chat draws it.
fn diffs(name: &str, input: &ToolInput) -> Vec<FileDiff> {
    fn text(s: Option<&String>) -> &str {
        s.map_or("", String::as_str)
    }
    let (old, new) = match name {
        "Edit" => (
            text(input.old_string.as_ref()),
            text(input.new_string.as_ref()),
        ),
        "Write" => ("", text(input.content.as_ref())),
        _ => return Vec::new(),
    };
    vec![FileDiff {
        path: text(input.file_path.as_ref()).to_string(),
        text: difftext::changed(old, new),
    }]
}

/// The entries of one conversation, newest last, and which changed since last asked.
#[derive(Default)]
pub(super) struct Mapper {
    kept: VecDeque<Entry>,
    /// Older entries were left out: dropped at the cap, or never read.
    earlier: bool,
    /// Ids added or changed since [`Mapper::take_touched`], in order of first touch.
    touched: Vec<String>,
    touched_ids: HashSet<String>,
}

impl Mapper {
    pub(super) fn kept(&self) -> Vec<Entry> {
        self.kept.iter().cloned().collect()
    }

    pub(super) fn earlier(&self) -> bool {
        self.earlier
    }

    pub(super) fn mark_earlier(&mut self) {
        self.earlier = true;
    }

    /// The entries added or changed since the last call, each once, in order of first touch.
    pub(super) fn take_touched(&mut self) -> Vec<Entry> {
        let by_id: HashMap<&str, &Entry> = self.kept.iter().map(|e| (e.id(), e)).collect();
        let entries = self
            .touched
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied().cloned())
            .collect();
        self.touched.clear();
        self.touched_ids.clear();
        entries
    }

    /// Add `entry`, or replace the kept one with its id.
    fn put(&mut self, entry: Entry) {
        let entry = capped(entry);
        let id = entry.id().to_string();
        if let Some(old) = self.kept.iter_mut().find(|e| e.id() == id) {
            *old = entry;
        } else {
            self.kept.push_back(entry);
            if self.kept.len() > KEPT {
                self.kept.pop_front();
                self.earlier = true;
            }
        }
        if self.touched_ids.insert(id.clone()) {
            self.touched.push(id);
        }
    }

    /// Map the line that starts `at` bytes into the transcript.
    pub(super) fn line(&mut self, at: u64, raw: &str) {
        if raw.len() > LINE_MAX {
            return;
        }
        let Ok(line) = serde_json::from_str::<Line>(raw) else {
            return;
        };
        if line.is_sidechain || line.is_meta {
            return;
        }
        let Some(message) = line.message else {
            return;
        };
        match line.kind.as_str() {
            "assistant" => self.assistant(at, message.content, line.is_api_error_message),
            "user" => {
                let human = line.origin.is_none_or(|o| o.kind == "human");
                self.user(at, message.content, human);
            }
            _ => {}
        }
    }

    fn assistant(&mut self, at: u64, content: Content, api_error: bool) {
        for (i, block) in content.blocks() {
            let id = format!("{at}-{i}");
            let entry = match block {
                Block::Text { text } if text.trim().is_empty() => continue,
                Block::Text { text } if api_error => Entry::Notice { id, text },
                Block::Text { text } => Entry::Agent { id, text },
                Block::Thinking { thinking } if thinking.trim().is_empty() => continue,
                Block::Thinking { thinking } => Entry::Thought { id, text: thinking },
                Block::ToolUse {
                    id: tool_id,
                    name,
                    input,
                } => Entry::Tool {
                    id: if tool_id.is_empty() {
                        id
                    } else {
                        format!("tool-{tool_id}")
                    },
                    title: title(&name, &input),
                    status: ToolStatus::Running,
                    detail: String::new(),
                    diffs: diffs(&name, &input),
                },
                Block::ToolResult { .. } | Block::Image {} | Block::Other => continue,
            };
            self.put(entry);
        }
    }

    fn user(&mut self, at: u64, content: Content, human: bool) {
        let mut prompt: Vec<String> = Vec::new();
        let mut first = None;
        for (i, block) in content.blocks() {
            if let Some(prose) = block.prose() {
                first.get_or_insert(i);
                prompt.push(prose);
                continue;
            }
            match block {
                Block::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => self.tool_result(&tool_use_id, content, is_error),
                Block::Text { .. }
                | Block::Image {}
                | Block::Thinking { .. }
                | Block::ToolUse { .. }
                | Block::Other => {}
            }
        }
        let text = prompt.join("\n");
        let Some(first) = first.filter(|_| !text.trim().is_empty()) else {
            return;
        };
        let id = format!("{at}-{first}");
        self.put(if human {
            Entry::User { id, text }
        } else {
            Entry::Notice {
                id,
                text: text.chars().take(NOTICE_MAX).collect(),
            }
        });
    }

    fn tool_result(&mut self, tool_use_id: &str, content: Content, is_error: bool) {
        if tool_use_id.is_empty() {
            return;
        }
        let id = format!("tool-{tool_use_id}");
        let Some(Entry::Tool { title, diffs, .. }) = self.kept.iter().find(|e| e.id() == id) else {
            return;
        };
        let entry = Entry::Tool {
            title: title.clone(),
            diffs: diffs.clone(),
            status: if is_error {
                ToolStatus::Failed
            } else {
                ToolStatus::Done
            },
            detail: crate::pty::plain_text(&content.text()),
            id,
        };
        self.put(entry);
    }
}
