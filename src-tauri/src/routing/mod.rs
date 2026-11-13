//! Provider routing: which connection a desk's requests may use, and what happens when one runs out.
//! Stage A is pure types and rules: no network, no listener, no clock.
//!
//! # How Stage B calls the state machine (`state::RouteState`)
//!
//! One `Mutex<RouteState>`, locked with `sync::lock`. It is a leaf lock: never take the Office
//! lock, the settings lock or the `Keys` lock while holding it, and never hold it across the
//! network, the Keychain, a file write or a sleep.
//!
//! Per request, on its own thread:
//!
//! 1. Work out `credentials` for the session's chain with `Keys` (the Keychain may block), with
//!    no route lock held.
//! 2. Lock, `begin`, `unsaved`, unlock.
//! 3. An error from `begin`: answer locally and call nothing. `Blocked` is an HTTP 400 with
//!    Anthropic's error envelope and a truthful sentence, never a 429 or 529.
//! 4. Write what `unsaved` returned with `ExhaustionFile::save` (its own lock), then
//!    `saved(revision)` under the route lock once the write went through. Then call upstream
//!    with the lease and relay, with no route lock held. Track `relayed`: true from the first
//!    byte of the model's reply that reaches the client.
//! 5. Lock, `settle`, `unsaved`, unlock. Save before going on, so a refusal is on disk before
//!    the client can ask again, then `saved(revision)` once the write went through.
//! 6. Act on the verdict. For `Again`, sleep `wait_ms` outside the lock, stop if the client has
//!    gone (settle the new lease as `Cancelled`), else go to 4.
//!
//! Every lease is settled exactly once: put the settle in a guard that reports `Cancelled` if
//! the thread unwinds. A probe lease that is never settled frees itself after
//! `availability::PROBE_LEASE_SECS`.
//!
//! `open` when a routed desk is launched or resumed, `close` when its terminal exits,
//! `apply_preferences` from `update_settings` when the routing group changed, `retry` and
//! `switch` from the route commands, each followed by `unsaved`, a save and, once it went
//! through, `saved(revision)`. At start-up: `ExhaustionFile::load`, then
//! `RouteState::new(records)`.

pub mod availability;
pub mod capabilities;
pub mod commands;
pub mod config;
#[cfg(test)]
pub(crate) mod fixtures;
pub mod floors;
pub mod keys;
pub mod observe;
pub mod policy;
pub mod records;
pub mod state;
pub mod status;
