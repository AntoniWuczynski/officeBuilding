//! Secrets: held in memory in a type that cannot be printed, kept in the macOS Keychain.

use security_framework::passwords::{
    PasswordOptions, delete_generic_password, generic_password, set_generic_password,
};
use std::fmt;

/// `errSecItemNotFound`: the Keychain has no item under that service and account.
const ITEM_NOT_FOUND: i32 = -25300;

/// A secret in memory. No `Display`, no `Serialize`, and `Debug` prints `Secret(..)`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The secret itself. Only for the call that sends it or stores it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

pub trait SecretStore: Send + Sync {
    /// None when nothing is saved under `account`.
    fn get(&self, account: &str) -> Result<Option<Secret>, String>;
    fn set(&self, account: &str, secret: &Secret) -> Result<(), String>;
    /// Removing what is not there is not an error.
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// The login keychain: one generic password per account under the app's service name.
pub struct Keychain {
    service: String,
}

impl Keychain {
    /// `service`: the app's bundle identifier.
    #[must_use]
    pub fn new(service: String) -> Self {
        Self { service }
    }
}

// The system's error text is an OSStatus message or code: it never holds the secret.
impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<Secret>, String> {
        match generic_password(PasswordOptions::new_generic_password(
            &self.service,
            account,
        )) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(|text| Some(Secret::new(text)))
                .map_err(|_| "the saved key is not text".to_string()),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(format!("the Keychain did not hand over the key ({e})")),
        }
    }

    fn set(&self, account: &str, secret: &Secret) -> Result<(), String> {
        set_generic_password(&self.service, account, secret.expose().as_bytes())
            .map_err(|e| format!("the Keychain did not save the key ({e})"))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match delete_generic_password(&self.service, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(format!("the Keychain did not remove the key ({e})")),
        }
    }
}

/// For tests: secrets in a map, an error to play, and a count of reads.
#[cfg(test)]
#[derive(Default)]
pub struct MemorySecrets {
    saved: std::sync::Mutex<std::collections::BTreeMap<String, Secret>>,
    failure: std::sync::Mutex<Option<String>>,
    reads: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl MemorySecrets {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start with `value` saved under `account`.
    #[must_use]
    pub fn with(self, account: &str, value: &str) -> Self {
        crate::sync::lock(&self.saved).insert(account.to_string(), Secret::new(value.into()));
        self
    }

    /// Every call from now on returns `message`.
    pub fn fail_with(&self, message: &str) {
        *crate::sync::lock(&self.failure) = Some(message.to_string());
    }

    /// How many `get` calls were made.
    pub fn reads(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn played(&self) -> Result<(), String> {
        crate::sync::lock(&self.failure).clone().map_or(Ok(()), Err)
    }
}

#[cfg(test)]
impl SecretStore for MemorySecrets {
    fn get(&self, account: &str) -> Result<Option<Secret>, String> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.played()?;
        Ok(crate::sync::lock(&self.saved).get(account).cloned())
    }

    fn set(&self, account: &str, secret: &Secret) -> Result<(), String> {
        self.played()?;
        crate::sync::lock(&self.saved).insert(account.to_string(), secret.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.played()?;
        crate::sync::lock(&self.saved).remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_the_value() {
        let shown = format!("{:?}", Secret::new("hunter2".into()));
        assert_eq!(shown, "Secret(..)");
        assert!(!shown.contains("hunter2"));
    }

    #[test]
    fn expose_hands_back_the_value() {
        assert_eq!(Secret::new("test-key".into()).expose(), "test-key");
    }

    #[test]
    fn memory_secrets_get_set_delete() {
        let store = MemorySecrets::new();
        assert_eq!(store.get("a"), Ok(None));
        store.set("a", &Secret::new("test-key".into())).unwrap();
        assert_eq!(store.get("a"), Ok(Some(Secret::new("test-key".into()))));
        store.delete("a").unwrap();
        assert_eq!(store.get("a"), Ok(None));
        assert_eq!(store.delete("a"), Ok(()));
        assert_eq!(store.reads(), 3);
    }

    #[test]
    fn memory_secrets_starts_with_a_saved_value() {
        let store = MemorySecrets::new().with("a", "test-key");
        assert_eq!(store.get("a"), Ok(Some(Secret::new("test-key".into()))));
        assert_eq!(store.get("b"), Ok(None));
    }

    #[test]
    fn memory_secrets_plays_a_failure_on_every_call() {
        let store = MemorySecrets::new().with("a", "test-key");
        store.fail_with("locked");
        assert_eq!(store.get("a"), Err("locked".into()));
        assert_eq!(
            store.set("a", &Secret::new("other".into())),
            Err("locked".into())
        );
        assert_eq!(store.delete("a"), Err("locked".into()));
        assert_eq!(store.reads(), 1);
    }
}
