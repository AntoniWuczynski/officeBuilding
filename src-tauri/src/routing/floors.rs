//! What a floor's Info panel shows about routing, and the check on the context it is given.

use super::config::ContextId;
use crate::settings::RoutingSettings;
use serde::Serialize;

/// What a floor's Info panel shows about routing (`FloorRouting` in types.ts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorRouting {
    /// The master switch.
    pub enabled: bool,
    /// The floor's context. None: not set, or the one it had is gone from Settings.
    pub context: Option<ContextId>,
    /// Every context in Settings, in order, for the picker.
    pub contexts: Vec<ContextName>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextName {
    pub id: ContextId,
    pub name: String,
}

impl FloorRouting {
    /// `stored`: the context the floor's record holds.
    #[must_use]
    pub fn of(settings: &RoutingSettings, stored: Option<ContextId>) -> Self {
        Self {
            enabled: settings.enabled,
            context: stored.filter(|id| settings.context(id).is_some()),
            contexts: settings
                .contexts
                .iter()
                .map(|c| ContextName {
                    id: c.id.clone(),
                    name: c.name.clone(),
                })
                .collect(),
        }
    }
}

/// The context `raw` names, if it is in Settings.
///
/// # Errors
/// When no context in Settings has that id.
pub fn known_context(settings: &RoutingSettings, raw: &str) -> Result<ContextId, String> {
    ContextId::parse(raw)
        .filter(|id| settings.context(id).is_some())
        .ok_or_else(|| "that account context is no longer in Settings".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::fixtures::{ctx, settings};

    fn names(view: &FloorRouting) -> Vec<(&str, &str)> {
        view.contexts
            .iter()
            .map(|c| (c.id.as_str(), c.name.as_str()))
            .collect()
    }

    #[test]
    fn of_lists_the_contexts_and_shows_only_a_known_stored_one() {
        let s = settings();
        let rows = [
            ("none stored", None, None),
            ("a known one", Some(ctx("home")), Some(ctx("home"))),
            ("one that is gone", Some(ctx("gone")), None),
        ];
        for (name, stored, want) in rows {
            let view = FloorRouting::of(&s, stored);
            assert_eq!(view.context, want, "{name}");
            assert_eq!(
                names(&view),
                vec![("work", "Work"), ("home", "Home")],
                "{name}"
            );
            assert!(view.enabled, "{name}");
        }
        let off = RoutingSettings {
            enabled: false,
            ..settings()
        };
        assert!(!FloorRouting::of(&off, None).enabled);
        assert_eq!(
            FloorRouting::of(&RoutingSettings::default(), Some(ctx("work"))),
            FloorRouting {
                enabled: false,
                context: None,
                contexts: vec![]
            }
        );
    }

    #[test]
    fn floor_routing_serialises_to_the_wire_shape() {
        let view = FloorRouting::of(&settings(), Some(ctx("work")));
        assert_eq!(
            serde_json::to_string(&view).expect("json"),
            r#"{"enabled":true,"context":"work","contexts":[{"id":"work","name":"Work"},{"id":"home","name":"Home"}]}"#
        );
    }

    #[test]
    fn known_context_accepts_only_a_context_in_settings() {
        let s = settings();
        let gone = Err("that account context is no longer in Settings".to_string());
        let rows = [
            ("known", "work", Ok(ctx("work"))),
            ("unknown", "elsewhere", gone.clone()),
            ("does not parse", "Work Stuff", gone),
        ];
        for (name, raw, want) in rows {
            assert_eq!(known_context(&s, raw), want, "{name}");
        }
    }
}
