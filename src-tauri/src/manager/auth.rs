//! The tokens that tie a manager's helper to its floor. In memory only, never printed.

use crate::secrets::Secret;
use crate::sync::lock;
use security_framework::random::SecRandom;
use std::sync::Mutex;

/// The name of the variable a manager's terminal carries its token in.
pub const TOKEN_ENV: &str = "OFFICE_BUILDING_MANAGER_TOKEN";

const TOKEN_BYTES: usize = 32;
const TOKEN_LEN: usize = TOKEN_BYTES * 2;
const HEX: &[u8; 16] = b"0123456789abcdef";

/// A new token: 32 bytes from the system's random source, as 64 lowercase hex characters.
pub fn new_token() -> Result<Secret, String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    SecRandom::default()
        .copy_bytes(&mut bytes)
        .map_err(|e| format!("the system gave no random bytes for a token ({e})"))?;
    let mut text = String::with_capacity(TOKEN_LEN);
    for byte in bytes {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(Secret::new(text))
}

/// Whether `text` has a token's shape: exactly 64 characters, each 0-9 or a-f.
#[must_use]
pub fn well_formed(text: &str) -> bool {
    text.len() == TOKEN_LEN && text.bytes().all(|b| HEX.contains(&b))
}

/// Whether two byte strings are equal, taking the same time whatever they hold.
#[must_use]
pub fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let acc = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(acc) == 0
}

/// The tokens of the managers running now, each with its floor's project id. In memory only.
#[derive(Default)]
pub struct Registry {
    tokens: Mutex<Vec<(String, Secret)>>,
}

impl Registry {
    /// Give `project_id` a new token, replacing any it had.
    pub fn admit(&self, project_id: &str) -> Result<Secret, String> {
        let token = new_token()?;
        let mut tokens = lock(&self.tokens);
        tokens.retain(|(floor, _)| floor != project_id);
        tokens.push((project_id.to_string(), token.clone()));
        Ok(token)
    }

    /// Forget `project_id`'s token.
    pub fn revoke(&self, project_id: &str) {
        lock(&self.tokens).retain(|(floor, _)| floor != project_id);
    }

    /// The floor `presented` belongs to. Compares with every token, without stopping at a match.
    #[must_use]
    pub fn floor_of(&self, presented: &str) -> Option<String> {
        let tokens = lock(&self.tokens);
        let mut found = None;
        for (floor, token) in tokens.iter() {
            if same(token.expose().as_bytes(), presented.as_bytes()) {
                found = Some(floor.clone());
            }
        }
        found
    }

    /// Whether `presented` is still `project_id`'s token.
    #[must_use]
    pub fn holds(&self, project_id: &str, presented: &str) -> bool {
        lock(&self.tokens).iter().any(|(floor, token)| {
            floor == project_id && same(token.expose().as_bytes(), presented.as_bytes())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_64_lowercase_hex_characters_and_two_differ() {
        let one = new_token().unwrap();
        let two = new_token().unwrap();
        assert!(well_formed(one.expose()), "a new token has a token's shape");
        assert!(well_formed(two.expose()), "a new token has a token's shape");
        assert!(one != two, "two tokens differ");
    }

    #[test]
    fn well_formed_wants_exactly_64_lowercase_hex() {
        assert!(well_formed(&"0123456789abcdef".repeat(4)));
        assert!(!well_formed(&"a".repeat(63)));
        assert!(!well_formed(&"a".repeat(65)));
        assert!(!well_formed(&"A".repeat(64)));
        assert!(!well_formed(&"g".repeat(64)));
        assert!(!well_formed(&format!("{} ", "a".repeat(63))));
        assert!(!well_formed(""));
    }

    #[test]
    fn same_compares_whole_strings() {
        assert!(same(b"abc", b"abc"));
        assert!(same(b"", b""));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"xbc"));
        assert!(!same(b"abc", b"abcd"));
        assert!(!same(b"abc", b""));
    }

    #[test]
    fn floor_of_finds_the_right_floor_among_several() {
        let registry = Registry::default();
        let one = registry.admit("p-1").unwrap();
        let two = registry.admit("p-2").unwrap();
        let three = registry.admit("p-3").unwrap();
        assert_eq!(registry.floor_of(two.expose()).as_deref(), Some("p-2"));
        assert_eq!(registry.floor_of(one.expose()).as_deref(), Some("p-1"));
        assert_eq!(registry.floor_of(three.expose()).as_deref(), Some("p-3"));
        assert_eq!(registry.floor_of(&"0".repeat(64)), None);
        assert_eq!(registry.floor_of(""), None);
    }

    #[test]
    fn admit_replaces_the_floors_old_token() {
        let registry = Registry::default();
        let old = registry.admit("p-1").unwrap();
        let new = registry.admit("p-1").unwrap();
        assert_eq!(registry.floor_of(old.expose()), None);
        assert_eq!(registry.floor_of(new.expose()).as_deref(), Some("p-1"));
        assert!(!registry.holds("p-1", old.expose()));
    }

    #[test]
    fn revoke_forgets_only_that_floor() {
        let registry = Registry::default();
        let one = registry.admit("p-1").unwrap();
        let two = registry.admit("p-2").unwrap();
        registry.revoke("p-1");
        assert_eq!(registry.floor_of(one.expose()), None);
        assert!(!registry.holds("p-1", one.expose()));
        assert!(registry.holds("p-2", two.expose()));
    }

    #[test]
    fn holds_wants_the_token_of_that_floor() {
        let registry = Registry::default();
        let one = registry.admit("p-1").unwrap();
        let two = registry.admit("p-2").unwrap();
        assert!(registry.holds("p-1", one.expose()));
        assert!(!registry.holds("p-1", two.expose()));
        assert!(!registry.holds("p-3", one.expose()));
    }
}
