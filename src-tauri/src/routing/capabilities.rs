//! What a request needs beyond plain text, and which connection kinds can carry it.

use super::config::{ConnectionKind, effort_name, model_name};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What a request needs beyond plain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    /// Tools the client runs itself (ordinary function calls).
    Tools,
    Images,
    Documents,
    /// Tools the provider runs (web search and the like).
    ServerTools,
}

/// What a translated route can carry (U1, U8, U10). Documents and tools the provider runs are
/// not carried.
const TRANSLATED: &[Capability] = &[Capability::Tools, Capability::Images];

/// Whether a connection of `kind` can carry `capability`. A kind that speaks the Messages API carries everything.
#[must_use]
pub fn supports(kind: ConnectionKind, capability: Capability) -> bool {
    !kind.translated() || TRANSLATED.contains(&capability)
}

/// What one request asks for. No content: only what selection needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirements {
    pub model: String,
    pub effort: Option<String>,
    pub capabilities: BTreeSet<Capability>,
}

impl Requirements {
    /// None when the model or the effort is not a valid name. These come from a request body,
    /// so they are checked here, at the boundary.
    #[must_use]
    pub fn new(
        model: &str,
        effort: Option<&str>,
        capabilities: BTreeSet<Capability>,
    ) -> Option<Self> {
        let effort = match effort {
            Some(raw) => Some(effort_name(raw)?),
            None => None,
        };
        Some(Self {
            model: model_name(model)?,
            effort,
            capabilities,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::config::{EFFORT_MAX, MODEL_MAX};

    #[test]
    fn supports_follows_the_kind() {
        use Capability::{Documents, Images, ServerTools, Tools};
        use ConnectionKind::{
            AnthropicApiKey, ChatGptSignIn, ClaudeSubscription, OpenAiApiKey, OpenRouter,
        };
        let all = [Tools, Images, Documents, ServerTools];
        let rows: [(ConnectionKind, [bool; 4]); 5] = [
            (ClaudeSubscription, [true; 4]),
            (AnthropicApiKey, [true; 4]),
            (OpenRouter, [true; 4]),
            (OpenAiApiKey, [true, true, false, false]),
            (ChatGptSignIn, [true, true, false, false]),
        ];
        for (kind, expected) in rows {
            for (capability, want) in all.into_iter().zip(expected) {
                assert_eq!(supports(kind, capability), want, "{kind:?} {capability:?}");
            }
        }
    }

    #[test]
    fn capabilities_serialise_kebab_case() {
        let all = [
            Capability::Tools,
            Capability::Images,
            Capability::Documents,
            Capability::ServerTools,
        ];
        let json = serde_json::to_string(&all).expect("serialise");
        assert_eq!(json, r#"["tools","images","documents","server-tools"]"#);
    }

    #[test]
    fn requirements_check_their_names() {
        let long_model = "m".repeat(MODEL_MAX + 1);
        let long_effort = "e".repeat(EFFORT_MAX + 1);
        let rows: [(&str, &str, Option<&str>, bool); 6] = [
            ("plain", "claude-opus-5-5", Some("high"), true),
            ("no effort", "claude-opus-5-5", None, true),
            ("a space in the model", "claude opus", None, false),
            ("an over-long model", &long_model, None, false),
            (
                "an over-long effort",
                "claude-opus-5-5",
                Some(&long_effort),
                false,
            ),
            ("an empty effort", "claude-opus-5-5", Some(" "), false),
        ];
        for (row, model, effort, ok) in rows {
            let got = Requirements::new(model, effort, BTreeSet::new());
            assert_eq!(got.is_some(), ok, "{row}");
        }
        let trimmed = Requirements::new(" gpt-5.6-sol ", Some(" xhigh "), BTreeSet::new());
        assert_eq!(
            trimmed,
            Some(Requirements {
                model: "gpt-5.6-sol".into(),
                effort: Some("xhigh".into()),
                capabilities: BTreeSet::new(),
            })
        );
    }
}
