//! The Routing group of Settings: account contexts, connections, tiers and their mappings.

use crate::routing::config::{
    self, AccountContext, CONNECTIONS_MAX, CONTEXTS_MAX, Connection, ConnectionId, ConnectionKind,
    ContextId, EFFORTS_PER_TARGET_MAX, MODELS_PER_TIER_MAX, TierId, Tiers,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

fn bad_name() -> String {
    format!("a name must be 1 to {} characters", config::NAME_MAX)
}

fn bad_model() -> String {
    format!(
        "a model name must be 1 to {} characters with no spaces",
        config::MODEL_MAX
    )
}

fn bad_effort() -> String {
    format!(
        "an effort name must be 1 to {} characters with no spaces",
        config::EFFORT_MAX
    )
}

/// The Routing group of Settings (`RoutingSettings` in types.ts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoutingSettings {
    /// The master switch. Off: every desk launches exactly as it does today.
    pub enabled: bool,
    pub contexts: Vec<AccountContext>,
    pub connections: Vec<Connection>,
    pub tiers: Tiers,
}

fn has_repeat<T: Ord>(items: impl IntoIterator<Item = T>) -> bool {
    let mut seen = BTreeSet::new();
    !items.into_iter().all(|item| seen.insert(item))
}

fn check_ids(settings: &RoutingSettings) -> Result<(), String> {
    if settings.contexts.len() > CONTEXTS_MAX {
        return Err(format!(
            "there can be at most {CONTEXTS_MAX} account contexts"
        ));
    }
    if settings.connections.len() > CONNECTIONS_MAX {
        return Err(format!(
            "there can be at most {CONNECTIONS_MAX} connections"
        ));
    }
    let valid = settings.contexts.iter().all(|c| c.id.is_valid())
        && settings
            .connections
            .iter()
            .all(|c| c.id.is_valid() && c.context.is_valid());
    if !valid {
        return Err("an account context or a connection has an id that is not valid".into());
    }
    if has_repeat(settings.contexts.iter().map(|c| &c.id)) {
        return Err("two account contexts share an id".into());
    }
    if has_repeat(settings.connections.iter().map(|c| &c.id)) {
        return Err("two connections share an id".into());
    }
    Ok(())
}

fn check_names(settings: &mut RoutingSettings) -> Result<(), String> {
    let names = settings
        .contexts
        .iter_mut()
        .map(|c| &mut c.name)
        .chain(settings.connections.iter_mut().map(|c| &mut c.name));
    for text in names {
        *text = config::name(text).ok_or_else(bad_name)?;
    }
    Ok(())
}

fn check_connection_links(settings: &RoutingSettings, was: &RoutingSettings) -> Result<(), String> {
    let contexts: BTreeSet<&ContextId> = settings.contexts.iter().map(|c| &c.id).collect();
    if !settings
        .connections
        .iter()
        .all(|c| contexts.contains(&c.context))
    {
        return Err("a connection belongs to an account context that does not exist".into());
    }
    let subscriptions = settings
        .connections
        .iter()
        .filter(|c| c.kind == ConnectionKind::ClaudeSubscription)
        .count();
    if subscriptions > 1 {
        return Err(
            "only one Claude subscription connection is possible, because this Mac has one Claude login"
                .into(),
        );
    }
    for connection in &settings.connections {
        if was
            .connection(&connection.id)
            .is_some_and(|old| old.kind != connection.kind || old.context != connection.context)
        {
            return Err(
                "a connection's kind and account context cannot be changed: remove it and add a new one"
                    .into(),
            );
        }
    }
    Ok(())
}

fn check_targets(connection: &mut Connection) -> Result<(), String> {
    let passes_through = connection.kind.passes_through();
    for tier in TierId::ALL {
        let Some(target) = connection.targets.get_mut(tier).as_mut() else {
            continue;
        };
        if passes_through {
            return Err("the Claude subscription connection takes no model names".into());
        }
        target.model = config::model_name(&target.model).ok_or_else(bad_model)?;
        if target.efforts.len() > EFFORTS_PER_TARGET_MAX {
            return Err(format!(
                "a tier can map at most {EFFORTS_PER_TARGET_MAX} efforts"
            ));
        }
        target.efforts = std::mem::take(&mut target.efforts)
            .into_iter()
            .map(|(from, to)| {
                Ok((
                    config::effort_name(&from).ok_or_else(bad_effort)?,
                    config::effort_name(&to).ok_or_else(bad_effort)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
    }
    Ok(())
}

fn check_chains(settings: &RoutingSettings) -> Result<(), String> {
    for context in &settings.contexts {
        let own = |id: &ConnectionId| {
            settings
                .connection(id)
                .is_some_and(|c| c.context == context.id)
        };
        if !context.chain.iter().all(own) {
            return Err("an account context's order names a connection that is not its own".into());
        }
        if has_repeat(&context.chain) {
            return Err("an account context's order lists a connection twice".into());
        }
    }
    Ok(())
}

fn check_tiers(tiers: &mut Tiers) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for tier in TierId::ALL {
        let names = tiers.get_mut(tier);
        if names.len() > MODELS_PER_TIER_MAX {
            return Err(format!(
                "a tier can list at most {MODELS_PER_TIER_MAX} model names"
            ));
        }
        for name in names.iter_mut() {
            *name = config::model_name(name).ok_or_else(bad_model)?;
            if !seen.insert(name.clone()) {
                return Err("a model name is listed under two tiers".into());
            }
        }
    }
    Ok(())
}

impl RoutingSettings {
    /// The same settings with every text trimmed and every unused opt-in cleared, or why they
    /// are not allowed. `was` is the settings being replaced.
    pub(super) fn checked(mut self, was: &Self) -> Result<Self, String> {
        check_ids(&self)?;
        check_names(&mut self)?;
        check_connection_links(&self, was)?;
        for connection in &mut self.connections {
            check_targets(connection)?;
        }
        check_chains(&self)?;
        check_tiers(&mut self.tiers)?;
        for connection in &mut self.connections {
            connection.paid &= connection.kind.metered();
            connection.experimental &= connection.kind.experimental();
        }
        Ok(self)
    }

    /// What a file may hold: itself when valid, else the default.
    pub(super) fn clamped(self) -> Self {
        self.checked(&Self::default()).unwrap_or_default()
    }

    #[must_use]
    pub fn connection(&self, id: &ConnectionId) -> Option<&Connection> {
        self.connections.iter().find(|c| &c.id == id)
    }

    #[must_use]
    pub fn context(&self, id: &ContextId) -> Option<&AccountContext> {
        self.contexts.iter().find(|c| &c.id == id)
    }
}

/// For `Settings::routing`: a group that does not parse loads as the default, so the other
/// groups survive.
pub(super) fn lenient<'de, D: serde::Deserializer<'de>>(d: D) -> Result<RoutingSettings, D::Error> {
    // `Value` is only the step between the file and the typed group. It goes no further.
    let raw = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(raw).unwrap_or_default())
}

/// Whether a settings file holds a routing group that loads as the default instead.
pub(super) fn dropped(text: &str) -> bool {
    #[derive(Deserialize)]
    struct Probe {
        routing: Option<serde_json::Value>,
    }
    let Ok(Probe {
        routing: Some(raw), ..
    }) = serde_json::from_str(text)
    else {
        return false;
    };
    serde_json::from_value::<RoutingSettings>(raw)
        .ok()
        .is_none_or(|group| group.checked(&RoutingSettings::default()).is_err())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::config::{Targets, TierTarget};
    use crate::routing::fixtures::{conn, ctx, settings};
    use crate::settings::{Settings, SettingsPatch, SettingsStore};
    use std::fs;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ob-settings-routing-{}-{name}", std::process::id()))
            .join("settings.json")
    }

    fn with_file(name: &str, text: &str) -> PathBuf {
        let path = temp_path(name);
        fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        fs::write(&path, text).expect("write");
        path
    }

    fn cleanup(path: &std::path::Path) {
        fs::remove_dir_all(path.parent().expect("dir")).expect("cleanup");
    }

    fn patch(routing: RoutingSettings) -> SettingsPatch {
        SettingsPatch {
            routing: Some(routing),
            ..SettingsPatch::default()
        }
    }

    fn connection_mut<'a>(s: &'a mut RoutingSettings, id: &str) -> &'a mut Connection {
        s.connections
            .iter_mut()
            .find(|c| c.id == conn(id))
            .expect("connection")
    }

    fn target(model: &str) -> TierTarget {
        TierTarget {
            model: model.into(),
            efforts: BTreeMap::new(),
        }
    }

    #[test]
    fn a_file_without_the_group_loads_off_and_empty() {
        let path = with_file("none", r#"{"notifications":{}}"#);
        assert_eq!(
            SettingsStore::open(path.clone()).get().routing,
            RoutingSettings::default()
        );
        cleanup(&path);
    }

    #[test]
    fn the_fixture_saves_and_reads_back_equal() {
        let path = temp_path("roundtrip");
        let store = SettingsStore::open(path.clone());
        let saved = store.update(patch(settings())).expect("update");
        assert_eq!(saved.routing, settings());
        assert_eq!(SettingsStore::open(path.clone()).get().routing, settings());
        cleanup(&path);
    }

    #[test]
    fn one_connection_serialises_to_the_documented_json() {
        let mut s = settings();
        let api = connection_mut(&mut s, "api");
        api.targets = Targets::default();
        api.targets.haiku = Some(TierTarget {
            model: "m-1".into(),
            efforts: [("xhigh".to_string(), "high".to_string())].into(),
        });
        let want: serde_json::Value = serde_json::from_str(
            r#"{ "id": "api", "name": "api", "kind": "anthropic-api-key", "context": "work",
            "paid": true, "experimental": false,
            "targets": { "fable": null, "opus": null, "sonnet": null,
              "haiku": { "model": "m-1", "efforts": { "xhigh": "high" } } } }"#,
        )
        .expect("json");
        let got = serde_json::to_value(connection_mut(&mut s, "api")).expect("value");
        assert_eq!(got, want);
        let group = serde_json::to_value(RoutingSettings::default()).expect("value");
        assert_eq!(
            group,
            serde_json::json!({ "enabled": false, "contexts": [], "connections": [],
              "tiers": { "fable": [], "opus": [], "sonnet": [], "haiku": [] } })
        );
    }

    type Mutation = fn(&mut RoutingSettings);

    fn refuses(rows: Vec<(&str, Mutation, &str)>) {
        for (label, mutate, message) in rows {
            let mut s = settings();
            mutate(&mut s);
            assert_eq!(
                s.checked(&settings()).err().as_deref(),
                Some(message),
                "{label}"
            );
        }
    }

    #[test]
    fn checked_refuses_ids_names_and_links() {
        refuses(vec![
            (
                "ninth context",
                |s| {
                    for n in 0..7 {
                        s.contexts.push(AccountContext {
                            id: ctx(&format!("extra-{n}")),
                            name: "Extra".into(),
                            chain: vec![],
                        });
                    }
                },
                "there can be at most 8 account contexts",
            ),
            (
                "25th connection",
                |s| {
                    for n in 0..20 {
                        let mut c = s.connections[1].clone();
                        c.id = conn(&format!("more-{n}"));
                        s.connections.push(c);
                    }
                },
                "there can be at most 24 connections",
            ),
            (
                "capital in a context id",
                |s| {
                    s.contexts[1].id = serde_json::from_str("\"Home\"").expect("id");
                },
                "an account context or a connection has an id that is not valid",
            ),
            (
                "two contexts one id",
                |s| s.contexts[1].id = ctx("work"),
                "two account contexts share an id",
            ),
            (
                "two connections one id",
                |s| s.connections[2].id = conn("api"),
                "two connections share an id",
            ),
            (
                "blank name",
                |s| s.contexts[0].name = "  ".into(),
                "a name must be 1 to 60 characters",
            ),
            (
                "blank connection name",
                |s| s.connections[0].name = String::new(),
                "a name must be 1 to 60 characters",
            ),
            (
                "missing context",
                |s| s.connections[1].context = ctx("gone"),
                "a connection belongs to an account context that does not exist",
            ),
            (
                "second subscription",
                |s| {
                    let mut c = s.connections[0].clone();
                    c.id = conn("sub-2");
                    c.context = ctx("home");
                    s.connections.push(c);
                },
                "only one Claude subscription connection is possible, because this Mac has one Claude login",
            ),
        ]);
    }

    #[test]
    fn checked_refuses_targets_chains_and_tiers() {
        refuses(vec![
            (
                "target on the subscription",
                |s| s.connections[0].targets.opus = Some(target("m")),
                "the Claude subscription connection takes no model names",
            ),
            (
                "model with a space",
                |s| s.connections[1].targets.opus = Some(target("m 1")),
                "a model name must be 1 to 200 characters with no spaces",
            ),
            (
                "nine efforts",
                |s| {
                    let t = s.connections[1].targets.opus.as_mut().expect("target");
                    t.efforts = (0..9)
                        .map(|n| (format!("e{n}"), "high".to_string()))
                        .collect();
                },
                "a tier can map at most 8 efforts",
            ),
            (
                "blank effort value",
                |s| {
                    let t = s.connections[1].targets.opus.as_mut().expect("target");
                    t.efforts = [("low".to_string(), " ".to_string())].into();
                },
                "an effort name must be 1 to 40 characters with no spaces",
            ),
            (
                "chain of another context",
                |s| s.contexts[1].chain.push(conn("api")),
                "an account context's order names a connection that is not its own",
            ),
            (
                "chain names nothing",
                |s| s.contexts[1].chain.push(conn("gone")),
                "an account context's order names a connection that is not its own",
            ),
            (
                "chain twice",
                |s| s.contexts[1].chain.push(conn("home-api")),
                "an account context's order lists a connection twice",
            ),
            (
                "17 names in a tier",
                |s| {
                    s.tiers.opus = (0..17).map(|n| format!("n{n}")).collect();
                },
                "a tier can list at most 16 model names",
            ),
            (
                "bad name in a tier",
                |s| s.tiers.opus.push("a b".into()),
                "a model name must be 1 to 200 characters with no spaces",
            ),
            (
                "name under two tiers",
                |s| s.tiers.haiku.push("claude-opus-5-5".into()),
                "a model name is listed under two tiers",
            ),
            (
                "name twice in a tier",
                |s| s.tiers.opus.push("gpt-5.6-sol".into()),
                "a model name is listed under two tiers",
            ),
        ]);
    }

    #[test]
    fn a_known_connection_cannot_change_kind_or_context() {
        let message = "a connection's kind and account context cannot be changed: remove it and add a new one";
        let rows: Vec<(&str, Mutation)> = vec![
            ("kind", |s| {
                connection_mut(s, "api").kind = ConnectionKind::OpenRouter;
            }),
            ("context", |s| {
                connection_mut(s, "api").context = ctx("home");
            }),
        ];
        for (label, mutate) in rows {
            let mut s = settings();
            mutate(&mut s);
            s.contexts[0].chain.retain(|c| c != &conn("api"));
            assert_eq!(
                s.checked(&settings()).err().as_deref(),
                Some(message),
                "{label}"
            );
        }
    }

    #[test]
    fn checked_normalises() {
        let mut s = settings();
        connection_mut(&mut s, "sub").paid = true;
        connection_mut(&mut s, "api").experimental = true;
        s.contexts[0].name = "  Work  ".into();
        s.connections[1].targets.opus = Some(target("  m-1 "));
        s.tiers.sonnet = vec![" x ".into()];
        let got = s.checked(&settings()).expect("valid");
        assert!(!connection_mut(&mut got.clone(), "sub").paid);
        assert!(!connection_mut(&mut got.clone(), "api").experimental);
        assert!(connection_mut(&mut got.clone(), "oai").experimental);
        assert_eq!(got.contexts[0].name, "Work");
        assert_eq!(got.connections[1].targets.opus, Some(target("m-1")));
        assert_eq!(got.tiers.sonnet, vec!["x".to_string()]);
    }

    #[test]
    fn a_refused_patch_changes_nothing() {
        let path = temp_path("refused");
        let store = SettingsStore::open(path.clone());
        let mut bad = settings();
        bad.contexts[1].chain.push(conn("home-api"));
        assert!(store.update(patch(bad)).is_err());
        assert_eq!(store.get(), Settings::default());
        assert!(!path.exists());
        cleanup_if_any(&path);
    }

    fn cleanup_if_any(path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            // Best effort: the directory may never have been created.
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn a_patch_with_an_unknown_kind_does_not_deserialise() {
        let text = r#"{"routing":{"connections":[{"id":"a","name":"A","kind":"carrier-pigeon","context":"c"}]}}"#;
        assert!(serde_json::from_str::<SettingsPatch>(text).is_err());
    }

    #[test]
    fn a_file_with_an_unknown_kind_loads_the_default_and_keeps_the_rest() {
        let text = r#"{"notifications":{"finished":false},"routing":{"enabled":true,
            "connections":[{"id":"a","name":"A","kind":"carrier-pigeon","context":"c"}]}}"#;
        let path = with_file("unknown-kind", text);
        let got = SettingsStore::open(path.clone()).get();
        assert_eq!(got.routing, RoutingSettings::default());
        assert!(!got.notifications.finished);
        assert_eq!(kept(&path), text);
        cleanup(&path);
    }

    #[test]
    fn a_file_with_an_invalid_group_loads_the_default() {
        let mut bad = settings();
        bad.contexts[1].chain.push(conn("home-api"));
        let text = serde_json::json!({ "notifications": { "finished": false }, "routing": bad });
        let path = with_file("invalid", &text.to_string());
        let got = SettingsStore::open(path.clone()).get();
        assert_eq!(got.routing, RoutingSettings::default());
        assert!(!got.notifications.finished);
        assert_eq!(kept(&path), text.to_string());
        cleanup(&path);
    }

    #[test]
    fn a_valid_or_missing_group_is_not_copied_aside() {
        let path = with_file(
            "kept-none",
            &serde_json::json!({ "routing": settings() }).to_string(),
        );
        let _ = SettingsStore::open(path.clone());
        assert!(
            !path
                .with_file_name("settings.routing.invalid.json")
                .exists()
        );
        let path = with_file("kept-missing", r#"{"notifications":{}}"#);
        let _ = SettingsStore::open(path.clone());
        assert!(
            !path
                .with_file_name("settings.routing.invalid.json")
                .exists()
        );
        cleanup(&path);
    }

    /// What was copied aside from `path`'s directory.
    fn kept(path: &std::path::Path) -> String {
        fs::read_to_string(path.with_file_name("settings.routing.invalid.json")).expect("copy")
    }
}
