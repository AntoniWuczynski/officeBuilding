//! The typed pieces of the Routing preferences: ids, connection kinds, tiers, targets and the text checks.
//! Nothing here is seeded: a model name, a connection or a context only ever comes from A Human.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const NAME_MAX: usize = 60;
pub const MODEL_MAX: usize = 200;
pub const EFFORT_MAX: usize = 40;
pub const ID_MAX: usize = 40;
pub const CONTEXTS_MAX: usize = 8;
pub const CONNECTIONS_MAX: usize = 24;
pub const MODELS_PER_TIER_MAX: usize = 16;
pub const EFFORTS_PER_TARGET_MAX: usize = 8;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// The id, if `raw` is 1 to `ID_MAX` characters of `a-z`, `0-9` and `-`.
            #[must_use]
            pub fn parse(raw: &str) -> Option<Self> {
                let id = Self(raw.to_string());
                id.is_valid().then_some(id)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// The same test as `parse`, for a value serde built.
            #[must_use]
            pub fn is_valid(&self) -> bool {
                (1..=ID_MAX).contains(&self.0.len())
                    && self
                        .0
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            }
        }
    };
}

id_type!(
    /// The id of an account context.
    ContextId
);
id_type!(
    /// The id of a connection.
    ConnectionId
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ConnectionKind {
    #[serde(rename = "claude-subscription")]
    ClaudeSubscription,
    #[serde(rename = "anthropic-api-key")]
    AnthropicApiKey,
    #[serde(rename = "openrouter")]
    OpenRouter,
    #[serde(rename = "openai-api-key")]
    OpenAiApiKey,
    #[serde(rename = "chatgpt-sign-in")]
    ChatGptSignIn,
}

impl ConnectionKind {
    /// Use costs money per request, so it needs the paid opt-in.
    #[must_use]
    pub const fn metered(self) -> bool {
        match self {
            Self::ClaudeSubscription | Self::ChatGptSignIn => false,
            Self::AnthropicApiKey | Self::OpenRouter | Self::OpenAiApiKey => true,
        }
    }

    /// The upstream does not speak the Messages API.
    #[must_use]
    pub const fn translated(self) -> bool {
        match self {
            Self::ClaudeSubscription | Self::AnthropicApiKey | Self::OpenRouter => false,
            Self::OpenAiApiKey | Self::ChatGptSignIn => true,
        }
    }

    /// Every translated route is experimental, and nothing else is, in this build.
    #[must_use]
    pub const fn experimental(self) -> bool {
        self.translated()
    }

    /// It has an API key in the Keychain.
    #[must_use]
    pub const fn holds_key(self) -> bool {
        match self {
            Self::ClaudeSubscription | Self::ChatGptSignIn => false,
            Self::AnthropicApiKey | Self::OpenRouter | Self::OpenAiApiKey => true,
        }
    }

    /// The request goes upstream untouched, so the connection has no targets.
    #[must_use]
    pub const fn passes_through(self) -> bool {
        matches!(self, Self::ClaudeSubscription)
    }

    /// It cannot be tried until the building holds a credential for it.
    #[must_use]
    pub const fn needs_credential(self) -> bool {
        !matches!(self, Self::ClaudeSubscription)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TierId {
    Fable,
    Opus,
    Sonnet,
    Haiku,
}

impl TierId {
    pub const ALL: [Self; 4] = [Self::Fable, Self::Opus, Self::Sonnet, Self::Haiku];
}

/// One value per tier. A key missing from a file loads as the default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, bound(deserialize = "T: Deserialize<'de> + Default"))]
pub struct PerTier<T> {
    pub fable: T,
    pub opus: T,
    pub sonnet: T,
    pub haiku: T,
}

impl<T> PerTier<T> {
    #[must_use]
    pub const fn get(&self, tier: TierId) -> &T {
        match tier {
            TierId::Fable => &self.fable,
            TierId::Opus => &self.opus,
            TierId::Sonnet => &self.sonnet,
            TierId::Haiku => &self.haiku,
        }
    }

    pub fn get_mut(&mut self, tier: TierId) -> &mut T {
        match tier {
            TierId::Fable => &mut self.fable,
            TierId::Opus => &mut self.opus,
            TierId::Sonnet => &mut self.sonnet,
            TierId::Haiku => &mut self.haiku,
        }
    }

    /// The values in `TierId::ALL` order.
    pub fn iter(&self) -> impl Iterator<Item = (TierId, &T)> {
        TierId::ALL.into_iter().map(|tier| (tier, self.get(tier)))
    }
}

/// The model names a desk asks for, by tier. Matched exactly.
pub type Tiers = PerTier<Vec<String>>;
/// What each tier is sent as on one connection. None: the connection does not serve that tier.
pub type Targets = PerTier<Option<TierTarget>>;

impl Tiers {
    /// The tier whose list holds exactly `model`.
    #[must_use]
    pub fn tier_of(&self, model: &str) -> Option<TierId> {
        self.iter()
            .find(|(_, names)| names.iter().any(|name| name == model))
            .map(|(tier, _)| tier)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierTarget {
    /// The model name sent upstream.
    pub model: String,
    /// The effort a desk asks for, to the effort sent upstream.
    #[serde(default)]
    pub efforts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: ConnectionId,
    pub name: String,
    pub kind: ConnectionKind,
    pub context: ContextId,
    /// A Human lets this connection spend money. Read only for a metered kind.
    #[serde(default)]
    pub paid: bool,
    /// A Human lets this experimental route be used. Read only for an experimental kind.
    #[serde(default)]
    pub experimental: bool,
    #[serde(default)]
    pub targets: Targets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountContext {
    pub id: ContextId,
    pub name: String,
    /// The order its connections are tried in, for Claude Code desks.
    #[serde(default)]
    pub chain: Vec<ConnectionId>,
}

fn trimmed_if(raw: &str, max: usize, ok: impl Fn(char) -> bool) -> Option<String> {
    let raw = raw.trim();
    ((1..=max).contains(&raw.chars().count()) && raw.chars().all(ok)).then(|| raw.to_string())
}

/// A context's or a connection's name, trimmed.
#[must_use]
pub fn name(raw: &str) -> Option<String> {
    trimmed_if(raw, NAME_MAX, |c| !c.is_control())
}

/// A model name, trimmed.
#[must_use]
pub fn model_name(raw: &str) -> Option<String> {
    trimmed_if(raw, MODEL_MAX, |c| c.is_ascii_graphic())
}

/// An effort name, trimmed.
#[must_use]
pub fn effort_name(raw: &str) -> Option<String> {
    trimmed_if(raw, EFFORT_MAX, |c| c.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Check = fn(&str) -> Option<String>;

    #[test]
    fn ids_parse_only_when_valid() {
        let long = "a".repeat(41);
        let max = "a".repeat(40);
        let rows: [(&str, &str, bool); 10] = [
            ("simple", "ctx-1a2b", true),
            ("digits", "1", true),
            ("max length", &max, true),
            ("empty", "", false),
            ("41 chars", &long, false),
            ("upper case", "Ctx", false),
            ("space", "a b", false),
            ("slash", "a/b", false),
            ("dot", "a.b", false),
            ("underscore", "a_b", false),
        ];
        for (name, raw, ok) in rows {
            assert_eq!(ContextId::parse(raw).is_some(), ok, "context {name}");
            assert_eq!(ConnectionId::parse(raw).is_some(), ok, "connection {name}");
        }
        let id = ContextId::parse("work").expect("id");
        assert_eq!(id.as_str(), "work");
        let built: ContextId = serde_json::from_str("\"Bad Id\"").expect("serde builds it");
        assert!(!built.is_valid());
    }

    #[test]
    fn kinds_match_the_method_table() {
        use ConnectionKind::{
            AnthropicApiKey, ChatGptSignIn, ClaudeSubscription, OpenAiApiKey, OpenRouter,
        };
        // metered, translated, experimental, holds_key, passes_through, needs_credential
        let rows: [(ConnectionKind, [bool; 6]); 5] = [
            (
                ClaudeSubscription,
                [false, false, false, false, true, false],
            ),
            (AnthropicApiKey, [true, false, false, true, false, true]),
            (OpenRouter, [true, false, false, true, false, true]),
            (OpenAiApiKey, [true, true, true, true, false, true]),
            (ChatGptSignIn, [false, true, true, false, false, true]),
        ];
        for (kind, want) in rows {
            let got = [
                kind.metered(),
                kind.translated(),
                kind.experimental(),
                kind.holds_key(),
                kind.passes_through(),
                kind.needs_credential(),
            ];
            assert_eq!(got, want, "{kind:?}");
        }
    }

    #[test]
    fn wire_names_round_trip() {
        let kinds = [
            (ConnectionKind::ClaudeSubscription, "claude-subscription"),
            (ConnectionKind::AnthropicApiKey, "anthropic-api-key"),
            (ConnectionKind::OpenRouter, "openrouter"),
            (ConnectionKind::OpenAiApiKey, "openai-api-key"),
            (ConnectionKind::ChatGptSignIn, "chatgpt-sign-in"),
        ];
        for (kind, wire) in kinds {
            let json = format!("\"{wire}\"");
            assert_eq!(serde_json::to_string(&kind).expect("to"), json, "{wire}");
            assert_eq!(
                serde_json::from_str::<ConnectionKind>(&json).expect("from"),
                kind
            );
        }
        let tiers = [
            (TierId::Fable, "fable"),
            (TierId::Opus, "opus"),
            (TierId::Sonnet, "sonnet"),
            (TierId::Haiku, "haiku"),
        ];
        for (tier, wire) in tiers {
            let json = format!("\"{wire}\"");
            assert_eq!(serde_json::to_string(&tier).expect("to"), json, "{wire}");
            assert_eq!(serde_json::from_str::<TierId>(&json).expect("from"), tier);
        }
        assert_eq!(tiers.map(|(tier, _)| tier), TierId::ALL);
    }

    #[test]
    fn text_checks() {
        let n61 = "n".repeat(61);
        let n60 = "n".repeat(60);
        let m201 = "m".repeat(201);
        let m200 = "m".repeat(200);
        let e41 = "e".repeat(41);
        let e40 = "e".repeat(40);
        let rows: [(&str, Check, &str, Option<&str>); 20] = [
            ("name trimmed", name, "  Work ", Some("Work")),
            ("name with space inside", name, "My work", Some("My work")),
            ("name empty", name, "   ", None),
            ("name max", name, &n60, Some(&n60)),
            ("name too long", name, &n61, None),
            ("name control", name, "a\tb", None),
            ("model trimmed", model_name, " m-1 ", Some("m-1")),
            ("model empty", model_name, "", None),
            ("model max", model_name, &m200, Some(&m200)),
            ("model too long", model_name, &m201, None),
            ("model control", model_name, "m\u{7}1", None),
            ("model inner space", model_name, "m 1", None),
            ("model non-ascii", model_name, "mödel", None),
            ("effort trimmed", effort_name, " high ", Some("high")),
            ("effort empty", effort_name, " ", None),
            ("effort max", effort_name, &e40, Some(&e40)),
            ("effort too long", effort_name, &e41, None),
            ("effort control", effort_name, "hi\ngh", None),
            ("effort inner space", effort_name, "very high", None),
            ("effort slash ok", effort_name, "a/b", Some("a/b")),
        ];
        for (label, check, raw, want) in rows {
            assert_eq!(check(raw).as_deref(), want, "{label}");
        }
    }

    #[test]
    fn tier_of_matches_exactly() {
        let tiers = crate::routing::fixtures::settings().tiers;
        let rows = [
            ("exact", "claude-opus-5-5", Some(TierId::Opus)),
            ("other vendor", "gpt-5.6-luna", Some(TierId::Haiku)),
            ("suffix", "claude-opus-5-5-x", None),
            ("prefix", "opus-5-5", None),
            ("case", "Claude-Opus-5-5", None),
            ("empty", "", None),
        ];
        for (label, model, want) in rows {
            assert_eq!(tiers.tier_of(model), want, "{label}");
        }
    }

    #[test]
    fn per_tier_fills_missing_keys_and_iterates_in_order() {
        let tiers: Tiers = serde_json::from_str(r#"{"opus":["a"],"haiku":["b"]}"#).expect("tiers");
        assert!(tiers.fable.is_empty() && tiers.sonnet.is_empty());
        let got: Vec<(TierId, usize)> = tiers.iter().map(|(t, v)| (t, v.len())).collect();
        assert_eq!(
            got,
            [
                (TierId::Fable, 0),
                (TierId::Opus, 1),
                (TierId::Sonnet, 0),
                (TierId::Haiku, 1)
            ]
        );
        let targets: Targets = serde_json::from_str("{}").expect("targets");
        assert!(targets.iter().all(|(_, t)| t.is_none()));
        let mut targets = targets;
        *targets.get_mut(TierId::Sonnet) = Some(TierTarget {
            model: "m".into(),
            efforts: BTreeMap::new(),
        });
        assert!(targets.get(TierId::Sonnet).is_some() && targets.get(TierId::Opus).is_none());
    }
}
