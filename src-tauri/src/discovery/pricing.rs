//! Dollar cost of the tokens a session used, at public API list prices. A
//! subscription (Claude Max, `ChatGPT` Plus) bills differently, so this is what
//! the same work would cost through the API, not what the user paid.
//!
//! Sources, read 2026-09-29:
//! * Anthropic: <https://platform.claude.com/docs/en/about-claude/pricing>
//!   (model table, cache multipliers, fast mode, data residency, web search).
//! * `OpenAI`: <https://developers.openai.com/api/docs/pricing> (standard tier,
//!   short and long context) and
//!   <https://developers.openai.com/api/docs/guides/prompt-caching> ("The input
//!   tokens total includes ordinary input, cached, and cache-write tokens").
//!
//! A model missing from these tables is not guessed at: its tokens are counted
//! as unpriced, and the UI says so.

use crate::model::Spend;
use serde::Deserialize;

const MTOK: f64 = 1_000_000.0;

/// `n` tokens in millions. Splitting into u32 halves converts without a lossy
/// `as` cast, exactly for any count below 2^53.
fn millions(n: u64) -> f64 {
    let high = u32::try_from(n >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(n & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    (f64::from(high) * 4_294_967_296.0 + f64::from(low)) / MTOK
}

/// Anthropic rates in USD per million tokens. Cache writes are 1.25x (5 minute)
/// and 2x (1 hour) the input rate; a cache read is `read_multiplier` x input.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ClaudeRates {
    input: f64,
    output: f64,
    read_multiplier: f64,
    /// Fast mode's (input, output), for the models that have it.
    fast: Option<(f64, f64)>,
}

const fn claude(
    input: f64,
    output: f64,
    read_multiplier: f64,
    fast: Option<(f64, f64)>,
) -> ClaudeRates {
    ClaudeRates {
        input,
        output,
        read_multiplier,
        fast,
    }
}

/// `claude-opus-5-5`, `claude-haiku-4-5-20251001`, `opus[1m]` → `opus-5-5`,
/// `haiku-4-5`, `opus`: the family-version key the table is written in.
fn claude_key(model: &str) -> &str {
    let m = model.strip_prefix("claude-").unwrap_or(model);
    let m = m.split('[').next().unwrap_or(m);
    match m.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => m,
    }
}

fn claude_rates(model: &str) -> Option<ClaudeRates> {
    Some(match claude_key(model) {
        "fable-5-1" | "mythos-5-1" => claude(10.0, 50.0, 0.025, None),
        "fable-5" | "mythos-5" => claude(10.0, 50.0, 0.1, None),
        "opus-5-5" => claude(4.0, 20.0, 0.05, Some((8.0, 40.0))),
        "opus-5" | "opus-4-8" => claude(5.0, 25.0, 0.1, Some((10.0, 50.0))),
        "opus-4-7" | "opus-4-6" | "opus-4-5" => claude(5.0, 25.0, 0.1, None),
        "opus-4-1" | "opus-4" => claude(15.0, 75.0, 0.1, None),
        "sonnet-5-5" | "sonnet-5" => claude(2.0, 10.0, 0.1, None),
        "sonnet-4-6" | "sonnet-4-5" | "sonnet-4" => claude(3.0, 15.0, 0.1, None),
        "haiku-4-5" => claude(1.0, 5.0, 0.1, None),
        "haiku-3-5" => claude(0.8, 4.0, 0.1, None),
        _ => return None,
    })
}

/// A Claude API message's `usage`, as Claude Code records it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ClaudeUsage {
    input_tokens: u64,
    cache_creation_input_tokens: u64,
    cache_read_input_tokens: u64,
    output_tokens: u64,
    cache_creation: Option<CacheCreation>,
    speed: Option<Speed>,
    inference_geo: Option<InferenceGeo>,
    server_tool_use: ServerToolUse,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(default)]
struct CacheCreation {
    ephemeral_5m_input_tokens: u64,
    ephemeral_1h_input_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Speed {
    Fast,
    #[serde(other)]
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum InferenceGeo {
    Us,
    #[serde(other)]
    Global,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(default)]
struct ServerToolUse {
    web_search_requests: u64,
}

/// One Claude API message's `usage`, priced for `model`. Tokens are input +
/// cache writes + output (cache reads excluded), as the office has always
/// counted them; the dollars include cache reads.
pub fn claude_cost(model: Option<&str>, usage: &ClaudeUsage) -> Spend {
    let written = usage.cache_creation_input_tokens;
    let tokens = usage.input_tokens + written + usage.output_tokens;
    let Some(rates) = model.and_then(claude_rates) else {
        return Spend {
            usd: 0.0,
            tokens,
            unpriced_tokens: tokens,
        };
    };
    // Without the per-TTL breakdown every write is taken as 5 minute, the API's default TTL.
    let (five_minute, one_hour) = match usage.cache_creation {
        Some(split) => (
            split.ephemeral_5m_input_tokens,
            split.ephemeral_1h_input_tokens,
        ),
        None => (written, 0),
    };
    let (rate_in, rate_out) = match rates.fast {
        Some(fast_rates) if usage.speed == Some(Speed::Fast) => fast_rates,
        _ => (rates.input, rates.output),
    };
    let mut usd = millions(usage.input_tokens) * rate_in
        + millions(five_minute) * rate_in * 1.25
        + millions(one_hour) * rate_in * 2.0
        + millions(usage.cache_read_input_tokens) * rate_in * rates.read_multiplier
        + millions(usage.output_tokens) * rate_out;
    if usage.inference_geo == Some(InferenceGeo::Us) {
        usd *= 1.1;
    }
    // $10 per 1,000 searches.
    usd += millions(usage.server_tool_use.web_search_requests) * 10_000.0;
    Spend {
        usd,
        tokens,
        unpriced_tokens: 0,
    }
}

/// `OpenAI` rates in USD per million tokens: (input, cached input, cache write, output).
/// A model with no listed cache-write rate bills cache writes as plain input.
type OpenAiRates = (f64, f64, Option<f64>, f64);

/// (short context ≤272K input tokens, long context when `OpenAI` lists one).
fn codex_rates(model: &str) -> Option<(OpenAiRates, Option<OpenAiRates>)> {
    Some(match model {
        "gpt-6-astra" => (
            (10.0, 1.0, Some(12.5), 50.0),
            Some((20.0, 2.0, Some(25.0), 75.0)),
        ),
        "gpt-6.1-sol" => (
            (2.0, 0.1, Some(2.5), 10.0),
            Some((4.0, 0.2, Some(5.0), 15.0)),
        ),
        "gpt-6-sol" => (
            (2.0, 0.2, Some(2.5), 10.0),
            Some((4.0, 0.4, Some(5.0), 15.0)),
        ),
        "gpt-6-luna" => (
            (0.1, 0.01, Some(0.125), 0.5),
            Some((0.2, 0.02, Some(0.25), 0.75)),
        ),
        "gpt-5.6-sol" => (
            (4.0, 0.4, Some(5.0), 20.0),
            Some((8.0, 0.8, Some(10.0), 30.0)),
        ),
        "gpt-5.6-terra" => (
            (2.0, 0.2, Some(2.5), 12.0),
            Some((4.0, 0.4, Some(5.0), 18.0)),
        ),
        "gpt-5.6-luna" => (
            (0.2, 0.02, Some(0.25), 1.2),
            Some((0.4, 0.04, Some(0.5), 1.8)),
        ),
        "gpt-5.5" => ((5.0, 0.5, None, 30.0), Some((10.0, 1.0, None, 45.0))),
        "gpt-5.4" => ((2.5, 0.25, None, 15.0), Some((5.0, 0.5, None, 22.5))),
        "gpt-5.4-mini" => ((0.75, 0.075, None, 4.5), None),
        "gpt-5.4-nano" => ((0.2, 0.02, None, 1.25), None),
        "gpt-5.3-codex" | "gpt-5.2" => ((1.75, 0.175, None, 14.0), None),
        "gpt-5.1" | "gpt-5" => ((1.25, 0.125, None, 10.0), None),
        "gpt-5-mini" => ((0.25, 0.025, None, 2.0), None),
        "gpt-5-nano" => ((0.05, 0.005, None, 0.4), None),
        _ => return None,
    })
}

/// Token counts from a Codex `token_count` event. `input` includes `cached`
/// and `cache_write`; `output` includes reasoning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CodexUsage {
    #[serde(rename = "input_tokens")]
    pub input: u64,
    #[serde(rename = "cached_input_tokens")]
    pub cached: u64,
    #[serde(rename = "cache_write_input_tokens")]
    pub cache_write: u64,
    #[serde(rename = "output_tokens")]
    pub output: u64,
}

impl CodexUsage {
    /// What was used since `earlier`. A total that went backwards restarted
    /// its count, so all of it is new.
    #[must_use]
    pub fn since(self, earlier: Self) -> Self {
        if self.input < earlier.input || self.output < earlier.output {
            return self;
        }
        Self {
            input: self.input - earlier.input,
            cached: self.cached.saturating_sub(earlier.cached),
            cache_write: self.cache_write.saturating_sub(earlier.cache_write),
            output: self.output - earlier.output,
        }
    }
}

/// Codex usage priced for `model`. `long_context` is whether the request that
/// produced it sent more than 272K input tokens. Tokens exclude cache reads,
/// matching Claude.
pub fn codex_cost(model: Option<&str>, delta: CodexUsage, long_context: bool) -> Spend {
    let fresh = delta
        .input
        .saturating_sub(delta.cached)
        .saturating_sub(delta.cache_write);
    let tokens = fresh + delta.cache_write + delta.output;
    let Some((short, long)) = model.and_then(codex_rates) else {
        return Spend {
            usd: 0.0,
            tokens,
            unpriced_tokens: tokens,
        };
    };
    let (rate_in, rate_cached, rate_write, rate_out) = match long {
        Some(long) if long_context => long,
        _ => short,
    };
    let usd = millions(fresh) * rate_in
        + millions(delta.cached) * rate_cached
        + millions(delta.cache_write) * rate_write.unwrap_or(rate_in)
        + millions(delta.output) * rate_out;
    Spend {
        usd,
        tokens,
        unpriced_tokens: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parsed(v: &serde_json::Value) -> ClaudeUsage {
        ClaudeUsage::deserialize(v).expect("usage")
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn model_ids_reduce_to_their_table_key() {
        assert_eq!(claude_key("claude-opus-5-5"), "opus-5-5");
        assert_eq!(claude_key("claude-haiku-4-5-20251001"), "haiku-4-5");
        assert_eq!(claude_key("opus[1m]"), "opus");
        assert_eq!(claude_key("claude-sonnet-5"), "sonnet-5");
    }

    #[test]
    fn opus_5_5_prices_every_token_kind() {
        // 1M of each: input $4, 5m write $5, 1h write $8, read $0.20, output $20.
        let usage = json!({
            "input_tokens": 1_000_000, "cache_creation_input_tokens": 2_000_000,
            "cache_read_input_tokens": 1_000_000, "output_tokens": 1_000_000,
            "cache_creation": {"ephemeral_5m_input_tokens": 1_000_000, "ephemeral_1h_input_tokens": 1_000_000}
        });
        let s = claude_cost(Some("claude-opus-5-5"), &parsed(&usage));
        assert!(close(s.usd, 4.0 + 5.0 + 8.0 + 0.2 + 20.0), "{}", s.usd);
        assert_eq!(s.tokens, 4_000_000);
        assert_eq!(s.unpriced_tokens, 0);
    }

    #[test]
    fn cache_writes_without_a_ttl_split_are_five_minute_writes() {
        let usage = json!({"cache_creation_input_tokens": 1_000_000});
        assert!(close(
            claude_cost(Some("claude-sonnet-5"), &parsed(&usage)).usd,
            2.5
        ));
    }

    #[test]
    fn fast_mode_us_inference_and_web_search_are_charged() {
        let fast = json!({"input_tokens": 1_000_000, "output_tokens": 1_000_000, "speed": "fast"});
        assert!(close(
            claude_cost(Some("claude-opus-5-5"), &parsed(&fast)).usd,
            48.0
        ));
        // Opus 4.7 has no fast mode, so "fast" leaves it at standard rates.
        assert!(close(
            claude_cost(Some("claude-opus-4-7"), &parsed(&fast)).usd,
            30.0
        ));
        let us = json!({"input_tokens": 1_000_000, "inference_geo": "us", "server_tool_use": {"web_search_requests": 3}});
        assert!(close(
            claude_cost(Some("claude-haiku-4-5-20251001"), &parsed(&us)).usd,
            1.1 + 0.03
        ));
    }

    #[test]
    fn an_unknown_model_is_unpriced_not_free() {
        let usage = json!({"input_tokens": 10, "output_tokens": 5});
        let s = claude_cost(Some("claude-something-9"), &parsed(&usage));
        assert_eq!((s.usd, s.tokens, s.unpriced_tokens), (0.0, 15, 15));
        assert_eq!(claude_cost(None, &parsed(&usage)).unpriced_tokens, 15);
    }

    #[test]
    fn codex_splits_input_into_fresh_cached_and_written() {
        let used = CodexUsage {
            input: 3_000_000,
            cached: 1_000_000,
            cache_write: 1_000_000,
            output: 1_000_000,
        };
        let s = codex_cost(Some("gpt-6-astra"), used, false);
        assert!(close(s.usd, 10.0 + 1.0 + 12.5 + 50.0), "{}", s.usd);
        assert_eq!(s.tokens, 3_000_000);
        let long = codex_cost(Some("gpt-6-astra"), used, true);
        assert!(close(long.usd, 20.0 + 2.0 + 25.0 + 75.0), "{}", long.usd);
    }

    #[test]
    fn codex_cache_writes_bill_as_input_when_no_write_rate_is_listed() {
        let used = CodexUsage {
            input: 1_000_000,
            cached: 0,
            cache_write: 1_000_000,
            output: 0,
        };
        assert!(close(codex_cost(Some("gpt-5.5"), used, false).usd, 5.0));
        // No long-context rate listed: short rates apply.
        assert!(close(
            codex_cost(Some("gpt-5.4-mini"), used, true).usd,
            0.75
        ));
    }

    #[test]
    fn codex_unknown_model_is_unpriced() {
        let used = CodexUsage {
            input: 100,
            cached: 40,
            cache_write: 0,
            output: 10,
        };
        let s = codex_cost(Some("gpt-reserve"), used, false);
        assert_eq!((s.usd, s.tokens, s.unpriced_tokens), (0.0, 70, 70));
    }

    #[test]
    fn codex_usage_since_takes_the_difference_or_restarts() {
        let a = CodexUsage {
            input: 100,
            cached: 50,
            cache_write: 0,
            output: 10,
        };
        let b = CodexUsage {
            input: 150,
            cached: 90,
            cache_write: 5,
            output: 12,
        };
        assert_eq!(
            b.since(a),
            CodexUsage {
                input: 50,
                cached: 40,
                cache_write: 5,
                output: 2
            }
        );
        assert_eq!(a.since(a), CodexUsage::default());
        assert_eq!(a.since(b), a);
    }
}
