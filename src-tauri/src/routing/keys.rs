//! API keys by connection: one Keychain item each, held in memory once read. The key never leaves
//! this module except through `get`, for the one call that sends it upstream.

use super::config::{ConnectionId, ConnectionKind};
use crate::secrets::{Secret, SecretStore};
use crate::settings::RoutingSettings;
use crate::sync::lock;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const KEY_MAX: usize = 500;

/// What the building knows about a connection's API key. Never the key itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum KeyStatus {
    Missing,
    /// A key is in the Keychain. It has not been checked with the vendor.
    Saved,
    /// The Keychain would not hand it over.
    Unavailable {
        reason: String,
    },
}

/// No `Debug`: the secret stays out of every printout.
enum Held {
    Missing,
    Saved(Secret),
    Unavailable(String),
}

impl Held {
    fn status(&self) -> KeyStatus {
        match self {
            Self::Missing => KeyStatus::Missing,
            Self::Saved(_) => KeyStatus::Saved,
            Self::Unavailable(reason) => KeyStatus::Unavailable {
                reason: reason.clone(),
            },
        }
    }
}

pub struct Keys {
    secrets: Arc<dyn SecretStore>,
    /// By Keychain account.
    held: Mutex<BTreeMap<String, Held>>,
}

impl Keys {
    #[must_use]
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            secrets,
            held: Mutex::new(BTreeMap::new()),
        }
    }

    /// Run `f` on what is held for `account`, reading the Keychain first if it has not been read
    /// this launch. The lock is held across that read on purpose: two threads must not both ask,
    /// because each ask can raise a system prompt.
    fn with_held<R>(&self, account: String, f: impl FnOnce(&Held) -> R) -> R {
        let mut held = lock(&self.held);
        let entry =
            held.entry(account)
                .or_insert_with_key(|account| match self.secrets.get(account) {
                    Ok(Some(secret)) => Held::Saved(secret),
                    Ok(None) => Held::Missing,
                    Err(why) => Held::Unavailable(why),
                });
        f(entry)
    }

    /// Reads the Keychain the first time a connection is asked about this launch, then answers
    /// from memory.
    pub fn status(&self, kind: ConnectionKind, id: &ConnectionId) -> KeyStatus {
        self.with_held(account(kind, id), Held::status)
    }

    /// Save `key`, or remove the saved one with None. The Keychain is written first, and memory
    /// only once it has taken the change. `listed` is asked under the lock whether the
    /// connection is still in Settings, so a save cannot land after `forget` has run.
    ///
    /// # Errors
    /// When the text does not look like a key, the connection has left Settings, or the Keychain
    /// refuses (its words, never the key).
    pub fn set(
        &self,
        kind: ConnectionKind,
        id: &ConnectionId,
        key: Option<String>,
        listed: impl FnOnce() -> bool,
    ) -> Result<KeyStatus, String> {
        let account = account(kind, id);
        let Some(key) = key else {
            let mut held = lock(&self.held);
            self.secrets.delete(&account)?;
            held.insert(account, Held::Missing);
            return Ok(KeyStatus::Missing);
        };
        let key = key.trim();
        if !(1..=KEY_MAX).contains(&key.chars().count())
            || !key.chars().all(|c| c.is_ascii_graphic())
        {
            return Err("that does not look like an API key".to_string());
        }
        let secret = Secret::new(key.to_string());
        let mut held = lock(&self.held);
        if !listed() {
            return Err(NOT_LISTED.to_string());
        }
        self.secrets.set(&account, &secret)?;
        held.insert(account, Held::Saved(secret));
        Ok(KeyStatus::Saved)
    }

    /// The key, for the one call that sends it upstream (Stage B). None: nothing is saved.
    ///
    /// # Errors
    /// When the Keychain would not hand it over.
    pub fn get(&self, kind: ConnectionKind, id: &ConnectionId) -> Result<Option<Secret>, String> {
        self.with_held(account(kind, id), |held| match held {
            Held::Missing => Ok(None),
            Held::Saved(secret) => Ok(Some(secret.clone())),
            Held::Unavailable(why) => Err(why.clone()),
        })
    }

    /// Remove the keys of connections that have left Settings. Best effort: a Keychain refusal is
    /// dropped, and the entry leaves memory either way.
    pub fn forget(&self, gone: &[(ConnectionKind, ConnectionId)]) {
        let mut held = lock(&self.held);
        for (kind, id) in gone {
            let account = account(*kind, id);
            // Best effort: nothing can use an item whose connection is gone (D3).
            let _ = self.secrets.delete(&account);
            held.remove(&account);
        }
    }
}

const NOT_LISTED: &str = "that connection is no longer in Settings";

/// The kind's name on the wire, as serde writes it.
const fn wire_name(kind: ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::ClaudeSubscription => "claude-subscription",
        ConnectionKind::AnthropicApiKey => "anthropic-api-key",
        ConnectionKind::OpenRouter => "openrouter",
        ConnectionKind::OpenAiApiKey => "openai-api-key",
        ConnectionKind::ChatGptSignIn => "chatgpt-sign-in",
    }
}

/// The Keychain account of a connection's key: "routing-key.<kind as serialised>.<id>".
fn account(kind: ConnectionKind, id: &ConnectionId) -> String {
    format!("routing-key.{}.{}", wire_name(kind), id.as_str())
}

/// The connections in `before` that are not in `after`, with their kind.
#[must_use]
pub fn gone(
    before: &RoutingSettings,
    after: &RoutingSettings,
) -> Vec<(ConnectionKind, ConnectionId)> {
    before
        .connections
        .iter()
        .filter(|c| after.connection(&c.id).is_none())
        .map(|c| (c.kind, c.id.clone()))
        .collect()
}

/// The kind of the connection `raw` names, if it is in Settings and holds a key.
///
/// # Errors
/// When no connection in Settings has that id, or its kind takes no key.
pub fn keyed(
    settings: &RoutingSettings,
    raw: &str,
) -> Result<(ConnectionKind, ConnectionId), String> {
    let connection = ConnectionId::parse(raw)
        .and_then(|id| settings.connection(&id))
        .ok_or(NOT_LISTED)?;
    if !connection.kind.holds_key() {
        return Err("that kind of connection takes no API key".to_string());
    }
    Ok((connection.kind, connection.id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::fixtures::{conn, settings};
    use crate::secrets::MemorySecrets;

    const KEY: &str = "sk-test-0123456789";
    const API: ConnectionKind = ConnectionKind::AnthropicApiKey;

    type Gone = Vec<(ConnectionKind, ConnectionId)>;

    fn fresh() -> (Keys, Arc<MemorySecrets>) {
        let store = Arc::new(MemorySecrets::new());
        let secrets: Arc<dyn SecretStore> = store.clone();
        (Keys::new(secrets), store)
    }

    #[test]
    fn a_key_is_missing_then_saved_then_handed_over() {
        let (keys, store) = fresh();
        let id = conn("api");
        assert_eq!(keys.status(API, &id), KeyStatus::Missing);
        assert_eq!(
            keys.set(API, &id, Some(format!("  {KEY} ")), || true),
            Ok(KeyStatus::Saved)
        );
        assert_eq!(keys.status(API, &id), KeyStatus::Saved);
        assert_eq!(keys.get(API, &id), Ok(Some(Secret::new(KEY.into()))));
        assert_eq!(
            store.get("routing-key.anthropic-api-key.api"),
            Ok(Some(Secret::new(KEY.into())))
        );
    }

    #[test]
    fn the_keychain_is_read_once_per_connection() {
        let store = Arc::new(MemorySecrets::new().with("routing-key.openrouter.router", KEY));
        let secrets: Arc<dyn SecretStore> = store.clone();
        let keys = Keys::new(secrets);
        for _ in 0..3 {
            assert_eq!(
                keys.status(ConnectionKind::OpenRouter, &conn("router")),
                KeyStatus::Saved
            );
        }
        assert_eq!(
            keys.get(ConnectionKind::OpenRouter, &conn("router")),
            Ok(Some(Secret::new(KEY.into())))
        );
        assert_eq!(store.reads(), 1);
        assert_eq!(keys.status(API, &conn("api")), KeyStatus::Missing);
        assert_eq!(store.reads(), 2);
    }

    #[test]
    fn removing_a_key_deletes_the_item_and_removing_nothing_is_fine() {
        let (keys, store) = fresh();
        let id = conn("api");
        assert_eq!(keys.set(API, &id, None, || true), Ok(KeyStatus::Missing));
        keys.set(API, &id, Some(KEY.into()), || true).expect("save");
        assert_eq!(keys.set(API, &id, None, || true), Ok(KeyStatus::Missing));
        assert_eq!(keys.status(API, &id), KeyStatus::Missing);
        assert_eq!(keys.get(API, &id), Ok(None));
        assert_eq!(store.get("routing-key.anthropic-api-key.api"), Ok(None));
    }

    #[test]
    fn text_that_is_not_a_key_is_refused_and_nothing_is_saved() {
        let long = "k".repeat(KEY_MAX + 1);
        let rows: [(&str, &str); 5] = [
            ("empty", ""),
            ("only spaces", "   "),
            ("a space inside", "sk-test 0123"),
            ("non-ascii", "sk-tést-0123"),
            ("501 characters", &long),
        ];
        for (name, raw) in rows {
            let (keys, store) = fresh();
            assert_eq!(
                keys.set(API, &conn("api"), Some(raw.into()), || true),
                Err("that does not look like an API key".to_string()),
                "{name}"
            );
            assert_eq!(
                store.get("routing-key.anthropic-api-key.api"),
                Ok(None),
                "{name}"
            );
            assert_eq!(keys.status(API, &conn("api")), KeyStatus::Missing, "{name}");
        }
        let (keys, _) = fresh();
        let max = "k".repeat(KEY_MAX);
        assert_eq!(
            keys.set(API, &conn("api"), Some(max), || true),
            Ok(KeyStatus::Saved)
        );
    }

    #[test]
    fn a_failing_keychain_says_so_and_changes_nothing() {
        let (keys, store) = fresh();
        let id = conn("api");
        assert_eq!(keys.status(API, &id), KeyStatus::Missing);
        store.fail_with("the Keychain is locked");
        assert_eq!(
            keys.set(API, &id, Some(KEY.into()), || true),
            Err("the Keychain is locked".to_string())
        );
        assert_eq!(
            keys.set(API, &id, None, || true),
            Err("the Keychain is locked".to_string())
        );
        assert_eq!(keys.status(API, &id), KeyStatus::Missing);

        let (keys, store) = fresh();
        store.fail_with("the Keychain is locked");
        let unavailable = KeyStatus::Unavailable {
            reason: "the Keychain is locked".into(),
        };
        assert_eq!(keys.status(API, &id), unavailable);
        assert_eq!(
            keys.get(API, &id),
            Err("the Keychain is locked".to_string())
        );
        assert_eq!(store.reads(), 1);
    }

    #[test]
    fn the_same_id_under_another_kind_is_another_item() {
        let (keys, store) = fresh();
        let id = conn("shared");
        keys.set(API, &id, Some(KEY.into()), || true).expect("save");
        assert_eq!(
            keys.status(ConnectionKind::OpenAiApiKey, &id),
            KeyStatus::Missing
        );
        assert_eq!(keys.get(ConnectionKind::OpenAiApiKey, &id), Ok(None));
        assert_eq!(store.get("routing-key.openai-api-key.shared"), Ok(None));
    }

    #[test]
    fn a_save_for_a_connection_that_has_left_settings_writes_nothing() {
        let (keys, store) = fresh();
        let id = conn("api");
        assert_eq!(
            keys.set(API, &id, Some(KEY.into()), || false),
            Err(NOT_LISTED.to_string())
        );
        assert_eq!(keys.status(API, &id), KeyStatus::Missing);
        assert_eq!(store.get(&account(API, &id)), Ok(None));
    }

    #[test]
    fn forget_removes_the_item_and_the_memory_even_when_the_keychain_fails() {
        let (keys, store) = fresh();
        keys.set(API, &conn("api"), Some(KEY.into()), || true)
            .expect("save");
        keys.set(API, &conn("home-api"), Some(KEY.into()), || true)
            .expect("save");
        keys.forget(&[(API, conn("api"))]);
        assert_eq!(store.get("routing-key.anthropic-api-key.api"), Ok(None));
        assert_eq!(keys.status(API, &conn("api")), KeyStatus::Missing);
        assert_eq!(keys.status(API, &conn("home-api")), KeyStatus::Saved);

        store.fail_with("the Keychain is locked");
        keys.forget(&[(API, conn("home-api"))]);
        // Memory was cleared, so the next ask goes to the Keychain again.
        assert_eq!(
            keys.status(API, &conn("home-api")),
            KeyStatus::Unavailable {
                reason: "the Keychain is locked".into()
            }
        );
    }

    #[test]
    fn the_account_uses_the_kind_as_serde_writes_it() {
        use ConnectionKind::{
            AnthropicApiKey, ChatGptSignIn, ClaudeSubscription, OpenAiApiKey, OpenRouter,
        };
        for kind in [
            ClaudeSubscription,
            AnthropicApiKey,
            OpenRouter,
            OpenAiApiKey,
            ChatGptSignIn,
        ] {
            let json = serde_json::to_string(&kind).expect("kind");
            assert_eq!(json, format!("\"{}\"", wire_name(kind)), "{kind:?}");
            assert_eq!(
                account(kind, &conn("x")),
                format!("routing-key.{}.x", wire_name(kind))
            );
        }
    }

    #[test]
    fn gone_lists_the_connections_that_left() {
        let full = settings();
        let mut one_less = full.clone();
        one_less.connections.retain(|c| c.id != conn("router"));
        for context in &mut one_less.contexts {
            context.chain.retain(|id| id != &conn("router"));
        }
        let empty = RoutingSettings::default();
        let rows: [(&str, &RoutingSettings, &RoutingSettings, Gone); 4] = [
            ("nothing gone", &full, &full, vec![]),
            (
                "one gone",
                &full,
                &one_less,
                vec![(ConnectionKind::OpenRouter, conn("router"))],
            ),
            (
                "all gone",
                &full,
                &empty,
                full.connections
                    .iter()
                    .map(|c| (c.kind, c.id.clone()))
                    .collect(),
            ),
            ("one added", &one_less, &full, vec![]),
        ];
        for (name, before, after, want) in rows {
            assert_eq!(gone(before, after), want, "{name}");
        }
    }

    #[test]
    fn keyed_finds_connections_that_hold_a_key() {
        let s = settings();
        let gone = Err("that connection is no longer in Settings".to_string());
        let rows = [
            ("unknown id", "nope", gone.clone()),
            ("id that does not parse", "Not An Id", gone),
            (
                "the subscription",
                "sub",
                Err("that kind of connection takes no API key".to_string()),
            ),
            ("an Anthropic key", "api", Ok((API, conn("api")))),
        ];
        for (name, raw, want) in rows {
            assert_eq!(keyed(&s, raw), want, "{name}");
        }
    }

    #[test]
    fn the_key_is_in_no_status_and_no_settings() {
        let (keys, _) = fresh();
        let status = keys
            .set(API, &conn("api"), Some(KEY.into()), || true)
            .expect("save");
        let shown = [
            format!("{status:?}"),
            serde_json::to_string(&status).expect("status"),
            format!("{:?}", keys.status(API, &conn("api"))),
            format!("{:?}", keys.get(API, &conn("api"))),
            serde_json::to_string(&settings()).expect("settings"),
        ];
        for text in shown {
            assert!(!text.contains(KEY), "{text}");
        }
        assert_eq!(
            serde_json::to_string(&status).expect("status"),
            r#"{"kind":"saved"}"#
        );
        assert_eq!(
            serde_json::to_string(&KeyStatus::Unavailable {
                reason: "locked".into()
            })
            .expect("status"),
            r#"{"kind":"unavailable","reason":"locked"}"#
        );
    }
}
