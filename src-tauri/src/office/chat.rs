//! The chat view of a desk: whether a tool has one, which terminals were hired
//! with it, and how a hire picks its view. The conversation itself is drawn
//! from `structured`'s model.

use std::collections::BTreeSet;

pub(super) mod composer;
pub(super) mod follow;
pub(super) mod views;

/// What the office remembers about chat desks.
#[derive(Default)]
pub(super) struct State {
    /// The app terminals hired with the chat view.
    pub(super) terminals: BTreeSet<String>,
}
