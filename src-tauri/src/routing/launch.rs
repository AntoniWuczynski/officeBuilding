//! What a routed launch adds to a Claude Code desk: the base address, the secret header line,
//! the `env` prefix and the proxy exclusion. Pure.
//!
//! The prefix goes on the typed command, so it wins over the user's shell profile, and holds no
//! secret. The header line goes in the terminal's own environment (E15). The proxy exclusion goes
//! in both: the terminal's environment gives it a value, and the prefix adds the loopback address
//! again to whatever the profile left, so a profile that sets its own cannot send the desk's
//! plain request to the router, with its login, through a proxy.

use super::state::RouteId;
use super::upstream::LOCAL_HEADER;
use crate::secrets::Secret;

pub const BASE_URL: &str = "ANTHROPIC_BASE_URL";
pub const CUSTOM_HEADERS: &str = "ANTHROPIC_CUSTOM_HEADERS";
pub const HINT_HEADERS: &str = "CLAUDE_CODE_GATEWAY_HINT_HEADERS";
/// Removed from a routed desk's environment, whatever set them (brief, "Never in a routed
/// child's environment").
pub const UNSET: [&str; 3] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
];
/// The router's own address, added to the proxy exclusion.
const LOOPBACK: &str = "127.0.0.1";

/// `http://127.0.0.1:<port>/s/<route id>`. The route id is correlation, not a secret.
#[must_use]
pub fn base_url(port: u16, id: &RouteId) -> String {
    format!("http://{LOOPBACK}:{port}/s/{}", id.as_str())
}

/// The value of `ANTHROPIC_CUSTOM_HEADERS`: `x-office-building-route: <secret>` (T7).
#[must_use]
pub fn header_line(secret: &Secret) -> Secret {
    Secret::new(format!("{LOCAL_HEADER}: {}", secret.expose()))
}

/// What goes before the launch command. `-u` options come before the assignments (T19), and it
/// ends with a space. The last two words are typed unquoted on purpose: zsh, bash and fish all
/// expand a variable inside double quotes, so each keeps the profile's exclusions and adds ours.
#[must_use]
pub fn prefix(base_url: &str, quote: &dyn Fn(&str) -> String) -> String {
    let mut words = vec!["env".to_owned()];
    for name in UNSET {
        words.extend(["-u".to_owned(), name.to_owned()]);
    }
    words.push(format!("{BASE_URL}={}", quote(base_url)));
    words.push(format!("{HINT_HEADERS}={}", quote("1")));
    for name in ["NO_PROXY", "no_proxy"] {
        words.push(format!("{name}=\"${name},{LOOPBACK}\""));
    }
    format!("{} ", words.join(" "))
}

/// The value for `NO_PROXY` and `no_proxy`: `inherited` with `127.0.0.1` added, unless it is `*`
/// or already lists it among its comma- or space-separated entries.
#[must_use]
pub fn no_proxy(inherited: Option<&str>) -> String {
    let Some(inherited) = inherited.filter(|value| !value.trim().is_empty()) else {
        return LOOPBACK.to_owned();
    };
    let listed = inherited
        .split([',', ' '])
        .map(str::trim)
        .any(|entry| entry == "*" || entry == LOOPBACK);
    if listed {
        inherited.to_owned()
    } else {
        format!("{inherited},{LOOPBACK}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::quote;

    const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const PROXY: &str = r#"NO_PROXY="$NO_PROXY,127.0.0.1" no_proxy="$no_proxy,127.0.0.1" "#;

    #[test]
    fn the_base_address_names_the_port_and_the_route() {
        assert_eq!(
            base_url(4321, &RouteId::new("abc123".to_owned())),
            "http://127.0.0.1:4321/s/abc123"
        );
    }

    #[test]
    fn the_header_line_carries_the_secret_and_never_prints_it() {
        let line = header_line(&Secret::new(SECRET.to_owned()));
        assert_eq!(line.expose(), format!("x-office-building-route: {SECRET}"));
        let printed = format!("{line:?}");
        assert_eq!(printed, "Secret(..)");
        assert!(!printed.contains(SECRET));
    }

    #[test]
    fn the_prefix_is_word_for_word_under_each_shell() {
        let base = "http://127.0.0.1:4321/s/abc123";
        let unsets = "env -u ANTHROPIC_API_KEY -u ANTHROPIC_AUTH_TOKEN \
                      -u CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC";
        let rows = [
            (
                "zsh",
                "/bin/zsh",
                base,
                format!(
                    "{unsets} ANTHROPIC_BASE_URL='{base}' CLAUDE_CODE_GATEWAY_HINT_HEADERS='1' {PROXY}"
                ),
            ),
            (
                "fish",
                "/opt/homebrew/bin/fish",
                base,
                format!(
                    "{unsets} ANTHROPIC_BASE_URL='{base}' CLAUDE_CODE_GATEWAY_HINT_HEADERS='1' {PROXY}"
                ),
            ),
            (
                "zsh, a quote is the shell's own quoting",
                "/bin/zsh",
                "a'b",
                format!(
                    "{unsets} ANTHROPIC_BASE_URL='a'\\''b' CLAUDE_CODE_GATEWAY_HINT_HEADERS='1' {PROXY}"
                ),
            ),
            (
                "fish, a quote is the shell's own quoting",
                "/opt/homebrew/bin/fish",
                "a'b",
                format!(
                    "{unsets} ANTHROPIC_BASE_URL='a\\'b' CLAUDE_CODE_GATEWAY_HINT_HEADERS='1' {PROXY}"
                ),
            ),
        ];
        for (name, shell, base, expected) in rows {
            let q = |s: &str| quote(shell, s);
            assert_eq!(prefix(base, &q), expected, "{name}");
        }
    }

    #[test]
    fn the_prefix_holds_only_names_the_base_address_and_one() {
        let base = "http://127.0.0.1:4321/s/abc123";
        let q = |s: &str| quote("/bin/zsh", s);
        let text = prefix(base, &q);
        let assigned = [
            format!("{BASE_URL}='{base}'"),
            format!("{HINT_HEADERS}='1'"),
            r#"NO_PROXY="$NO_PROXY,127.0.0.1""#.to_owned(),
            r#"no_proxy="$no_proxy,127.0.0.1""#.to_owned(),
        ];
        for word in text.split_whitespace() {
            let allowed = word == "env"
                || word == "-u"
                || UNSET.contains(&word)
                || assigned.iter().any(|a| a == word);
            assert!(allowed, "{word}");
        }
    }

    #[test]
    fn the_proxy_exclusion_adds_the_loopback_address_once() {
        let rows = [
            ("nothing inherited", None, "127.0.0.1"),
            ("blank", Some("  "), "127.0.0.1"),
            ("everything excluded", Some("*"), "*"),
            (
                "listed among commas",
                Some("localhost,127.0.0.1,.internal"),
                "localhost,127.0.0.1,.internal",
            ),
            (
                "listed among spaces",
                Some("localhost 127.0.0.1"),
                "localhost 127.0.0.1",
            ),
            (
                "another value",
                Some("localhost,.internal"),
                "localhost,.internal,127.0.0.1",
            ),
        ];
        for (name, inherited, expected) in rows {
            assert_eq!(no_proxy(inherited), expected, "{name}");
        }
    }
}
