//! The experimental route from a desk's Messages request to `OpenAI`'s Responses API, and back.
//! `messages` reads the request strictly into what the desk asked, `responses` builds the
//! Responses request from it, `sse` splits and frames server-sent events, and `reply` turns
//! `OpenAI`'s stream events into Anthropic's, and `pump` writes them out with the commit boundary.
//! Nothing here logs, prints or formats the conversation.

pub mod messages;
pub mod pump;
pub mod reply;
pub mod responses;
pub mod sse;

/// Why this route cannot carry a request. Each has one sentence in `answers::cannot_carry`.
/// No content: the only text from the request is a member or block-type name, checked by `label`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carry {
    /// `stream` is not `true` (F21).
    NotStreamed,
    /// `max_tokens` is 0: a request to warm Anthropic's prompt cache (F10).
    Prewarm,
    /// `context_management` (F6).
    ContextManagement,
    /// A `stop_sequences` list that is not empty.
    StopSequences,
    /// A top-level member that is neither carried nor left out.
    Member { name: String },
    /// A member of `output_config` other than `effort`.
    OutputSetting,
    /// A content block of a type this route does not carry.
    Block { kind: String },
    /// An image that is not base64 data of a documented type or a web address.
    ImageSource,
    /// A tool that is not a plain function tool: another `type`, `strict: true`,
    /// `defer_loading: true`, or a member the reader does not know.
    Tool,
    /// A `tool_choice` the reader does not know.
    ToolChoice,
    /// A tool call or tool result whose id is empty or longer than 64 characters (U1).
    CallId,
    /// Anything else the reader cannot follow: a repeated member, a wrong type, a member it does
    /// not know inside a message or a block.
    Unknown,
}

/// The longest name `label` lets into a sentence.
const LABEL_MAX: usize = 40;

/// `raw` when it is 1 to `max` bytes of ASCII letters, digits or the `extra` bytes, so a
/// provider's code and a request's member name pass the same guard (F19).
#[must_use]
pub fn token<'a>(raw: &'a str, max: usize, extra: &[u8]) -> Option<&'a str> {
    let fits = (1..=max).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || extra.contains(&b));
    fits.then_some(raw)
}

/// A member or block-type name fit for a sentence: 1 to 40 characters of ASCII letters, digits,
/// `_` or `-`. None: it is not, and the caller falls back to `Carry::Unknown`.
#[must_use]
pub fn label(raw: &str) -> Option<String> {
    token(raw, LABEL_MAX, b"_-").map(str::to_owned)
}
