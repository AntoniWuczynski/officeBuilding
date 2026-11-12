//! Handing notes to desks: what may be typed (`text`), whether a desk can take a
//! note now (`gate`), and the notes still waiting (`outbox`). Pure, and about
//! desks only: whoever sends the notes keeps its own records.

pub mod gate;
pub mod outbox;
pub mod text;
