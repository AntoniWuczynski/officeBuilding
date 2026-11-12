//! Notes waiting for their desks, oldest first. In memory only. Issue #6 adds
//! its composer's notes here too, with an origin of their own.

pub const PER_DESK_MAX: usize = 5;
pub const FLOOR_MAX: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    /// The floor (project id) it belongs to.
    pub floor: String,
    /// The sender's session id, or empty when it has none yet.
    pub from: String,
    /// The session id of the desk it is for.
    pub to: String,
    /// Typed in front of `text` so the desk knows who wrote. Empty for none.
    pub intro: String,
    pub text: String,
    pub queued_at: String,
}

#[derive(Debug, Default)]
pub struct Outbox {
    items: Vec<Item>,
}

impl Outbox {
    /// Err when `to` already has `PER_DESK_MAX` notes or its floor `FLOOR_MAX`.
    pub fn push(&mut self, item: Item) -> Result<(), String> {
        if self.items.iter().filter(|i| i.to == item.to).count() >= PER_DESK_MAX {
            return Err(format!(
                "that desk already has {PER_DESK_MAX} notes waiting"
            ));
        }
        if self.items.iter().filter(|i| i.floor == item.floor).count() >= FLOOR_MAX {
            return Err(format!("this floor already has {FLOOR_MAX} notes waiting"));
        }
        self.items.push(item);
        Ok(())
    }

    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.items.iter().any(|i| i.id == id)
    }

    /// The oldest note for each desk, in the order they were queued.
    #[must_use]
    pub fn heads(&self) -> Vec<&Item> {
        let mut heads: Vec<&Item> = Vec::new();
        for item in &self.items {
            if !heads.iter().any(|h| h.to == item.to) {
                heads.push(item);
            }
        }
        heads
    }

    pub fn take(&mut self, id: &str) -> Option<Item> {
        let at = self.items.iter().position(|i| i.id == id)?;
        Some(self.items.remove(at))
    }

    /// Remove and return every note `gone` is true for.
    pub fn take_all(&mut self, gone: impl Fn(&Item) -> bool) -> Vec<Item> {
        let (taken, kept) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|i| gone(i));
        self.items = kept;
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, floor: &str, to: &str) -> Item {
        Item {
            id: id.to_string(),
            floor: floor.to_string(),
            from: "m".to_string(),
            to: to.to_string(),
            intro: String::new(),
            text: format!("text {id}"),
            queued_at: "2026-11-12T10:00:00Z".to_string(),
        }
    }

    fn ids<'a>(items: impl IntoIterator<Item = &'a Item>) -> Vec<&'a str> {
        items.into_iter().map(|i| i.id.as_str()).collect()
    }

    #[test]
    fn keeps_order_and_gives_each_desks_oldest_note() {
        let mut outbox = Outbox::default();
        for (id, to) in [("n1", "a"), ("n2", "b"), ("n3", "a"), ("n4", "c")] {
            outbox.push(note(id, "f", to)).expect("push");
        }
        assert_eq!(ids(outbox.items()), ["n1", "n2", "n3", "n4"]);
        assert_eq!(ids(outbox.heads()), ["n1", "n2", "n4"]);
        assert!(outbox.contains("n3") && !outbox.contains("n9"));
        assert_eq!(outbox.take("n1").map(|i| i.id), Some("n1".to_string()));
        assert_eq!(outbox.take("n1"), None);
        assert_eq!(ids(outbox.heads()), ["n2", "n3", "n4"]);
    }

    #[test]
    fn a_desk_holds_at_most_five_notes() {
        let mut outbox = Outbox::default();
        for i in 0..PER_DESK_MAX {
            outbox.push(note(&format!("n{i}"), "f", "a")).expect("push");
        }
        assert_eq!(
            outbox.push(note("n9", "f", "a")),
            Err("that desk already has 5 notes waiting".to_string())
        );
        assert!(outbox.push(note("n10", "f", "b")).is_ok());
    }

    #[test]
    fn a_floor_holds_at_most_twenty_notes_and_another_floor_is_unaffected() {
        let mut outbox = Outbox::default();
        for i in 0..FLOOR_MAX {
            let to = format!("d{}", i % 4);
            outbox.push(note(&format!("n{i}"), "f", &to)).expect("push");
        }
        assert_eq!(
            outbox.push(note("x", "f", "d9")),
            Err("this floor already has 20 notes waiting".to_string())
        );
        assert!(outbox.push(note("y", "g", "e1")).is_ok());
        assert_eq!(outbox.items().len(), FLOOR_MAX + 1);
    }

    #[test]
    fn take_all_removes_what_matches_and_keeps_the_rest_in_order() {
        let mut outbox = Outbox::default();
        for (id, to) in [("n1", "a"), ("n2", "b"), ("n3", "a"), ("n4", "c")] {
            outbox.push(note(id, "f", to)).expect("push");
        }
        let taken = outbox.take_all(|i| i.to == "a");
        assert_eq!(ids(&taken), ["n1", "n3"]);
        assert_eq!(ids(outbox.items()), ["n2", "n4"]);
        assert!(outbox.take_all(|_| false).is_empty());
    }
}
