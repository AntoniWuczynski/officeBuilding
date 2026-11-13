//! Test data shared by every routing slice. The names are fixtures only, never product defaults.

use super::config::{
    AccountContext, Connection, ConnectionId, ConnectionKind, ContextId, PerTier, Targets,
    TierTarget, Tiers,
};
use crate::routing::router::Opened;
use crate::settings::RoutingSettings;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(crate) fn conn(id: &str) -> ConnectionId {
    ConnectionId::parse(id).expect("fixture connection id")
}

pub(crate) fn ctx(id: &str) -> ContextId {
    ContextId::parse(id).expect("fixture context id")
}

fn names(list: [&str; 2]) -> Vec<String> {
    list.map(String::from).to_vec()
}

fn target(model: &str, efforts: &[(&str, &str)]) -> TierTarget {
    TierTarget {
        model: model.to_string(),
        efforts: efforts
            .iter()
            .map(|(from, to)| ((*from).to_string(), (*to).to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn connection(
    id: &str,
    kind: ConnectionKind,
    context: &str,
    paid: bool,
    targets: Targets,
) -> Connection {
    Connection {
        id: conn(id),
        name: id.to_string(),
        kind,
        context: ctx(context),
        paid,
        experimental: kind.experimental(),
        targets,
    }
}

fn mapped(names: [&str; 4], efforts: &[(&str, &str)]) -> Targets {
    let [fable, opus, sonnet, haiku] = names.map(|n| Some(target(n, efforts)));
    PerTier {
        fable,
        opus,
        sonnet,
        haiku,
    }
}

const CLAUDE: [&str; 4] = [
    "claude-fable-5-2",
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
];
const OPENAI: [&str; 4] = [
    "gpt-6-astra",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
];

pub(crate) fn settings() -> RoutingSettings {
    let routed = CLAUDE.map(|n| format!("anthropic/{n}"));
    let routed = [&routed[0], &routed[1], &routed[2], &routed[3]].map(String::as_str);
    let openai_efforts = [
        ("low", "low"),
        ("medium", "medium"),
        ("high", "high"),
        ("xhigh", "high"),
    ];
    RoutingSettings {
        enabled: true,
        contexts: vec![
            AccountContext {
                id: ctx("work"),
                name: "Work".into(),
                chain: ["sub", "api", "router", "oai"].map(conn).to_vec(),
            },
            AccountContext {
                id: ctx("home"),
                name: "Home".into(),
                chain: vec![conn("home-api")],
            },
        ],
        connections: vec![
            connection(
                "sub",
                ConnectionKind::ClaudeSubscription,
                "work",
                false,
                Targets::default(),
            ),
            connection(
                "api",
                ConnectionKind::AnthropicApiKey,
                "work",
                true,
                mapped(CLAUDE, &[]),
            ),
            connection(
                "router",
                ConnectionKind::OpenRouter,
                "work",
                true,
                mapped(routed, &[]),
            ),
            connection(
                "oai",
                ConnectionKind::OpenAiApiKey,
                "work",
                true,
                mapped(OPENAI, &openai_efforts),
            ),
            connection(
                "home-api",
                ConnectionKind::AnthropicApiKey,
                "home",
                true,
                mapped(CLAUDE, &[]),
            ),
        ],
        tiers: Tiers {
            fable: names([CLAUDE[0], OPENAI[0]]),
            opus: names([CLAUDE[1], OPENAI[1]]),
            sonnet: names([CLAUDE[2], OPENAI[2]]),
            haiku: names([CLAUDE[3], OPENAI[3]]),
        },
    }
}

/// Every connection of `settings()` that needs a credential, as if each had one saved.
pub(crate) fn credentials() -> BTreeSet<ConnectionId> {
    settings()
        .connections
        .into_iter()
        .filter(|c| c.kind.needs_credential())
        .map(|c| c.id)
        .collect()
}

/// A fresh folder for one test's records file, named by `scope` and `name`.
pub(crate) fn temp_path(scope: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ob-{scope}-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir.join("routing-exhaustion.json")
}

/// The secret half of a routed launch's local header.
pub(crate) fn secret_of(opened: &Opened) -> String {
    let (_, secret) = opened
        .header
        .expose()
        .split_once(": ")
        .expect("name: value");
    secret.to_owned()
}
