//! What the handover remembers across a restart: the used-up subscriptions it has
//! proven and the handovers it has carried out. Pure apart from `HandoverFile`.

use super::{Tool, Window};
use crate::store::write_json_atomic;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// How long a record with no reset time is held.
pub const UNKNOWN_HOLD: i64 = 5 * 60 * 60;

/// The furthest reset taken as real, counted from now or from the record's own time: a
/// week and a day. A larger number (milliseconds, say) would be a record that never lapses.
pub const RESET_HORIZON: i64 = 8 * 24 * 60 * 60;

const KEEP_SECONDS: i64 = 7 * 24 * 60 * 60;
const KEEP_COUNT: usize = 200;
const FILE_VERSION: u32 = 1;

/// A proven used-up subscription: which tool, which window if known, the reset if known,
/// when the building read it and at which desk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsedUp {
    pub tool: Tool,
    pub window: Option<Window>,
    pub reset_at: Option<i64>,
    /// By the building's clock, when it first read the tool's record.
    pub seen_at: i64,
    /// The desk whose own transcript held it.
    pub session_id: String,
}

impl UsedUp {
    /// When the record lapses: the reset, or `seen_at` plus `UNKNOWN_HOLD`.
    #[must_use]
    pub fn until(&self) -> i64 {
        self.reset_at.unwrap_or(self.seen_at + UNKNOWN_HOLD)
    }

    #[must_use]
    pub fn live(&self, now: i64) -> bool {
        now < self.until()
    }

    /// The same limit: equal resets, or no reset on either side and seen within `UNKNOWN_HOLD`
    /// of each other.
    #[must_use]
    pub fn same_limit(&self, reset_at: Option<i64>, seen_at: i64) -> bool {
        match (self.reset_at, reset_at) {
            (Some(a), Some(b)) => a == b,
            (None, None) => (self.seen_at - seen_at).abs() < UNKNOWN_HOLD,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Outcome {
    /// The other tool was started. `session` is its session once discovery has bound one to its terminal.
    Started { session: Option<String> },
    /// It did not work. `ended`: the old session had already been ended.
    Failed { reason: String, ended: bool },
}

/// One handover the building carried out or tried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Done {
    /// "h-<n>"
    pub id: String,
    pub from_session: String,
    pub from: Tool,
    pub to: Tool,
    pub at: i64,
    pub window: Option<Window>,
    pub reset_at: Option<i64>,
    /// `seen_at` of the record it answered.
    pub seen_at: i64,
    pub outcome: Outcome,
}

/// Everything the handover remembers across a restart.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Book {
    used_up: Vec<UsedUp>,
    done: Vec<Done>,
    next_id: u64,
    dirty: bool,
}

impl Book {
    /// Record what a desk's own transcript proves. True when this desk has not shown this limit before.
    pub fn saw(&mut self, record: UsedUp) -> bool {
        let known = self.used_up.iter().any(|r| {
            r.session_id == record.session_id
                && r.tool == record.tool
                && r.same_limit(record.reset_at, record.seen_at)
        });
        if known {
            return false;
        }
        self.used_up.retain(|r| r.session_id != record.session_id);
        self.used_up.push(record);
        self.dirty = true;
        true
    }

    /// The desk's tool answered again: its own record goes.
    pub fn cleared(&mut self, session_id: &str) {
        let before = self.used_up.len();
        self.used_up.retain(|r| r.session_id != session_id);
        self.dirty |= self.used_up.len() != before;
    }

    /// The live record of `tool` that lasts longest.
    #[must_use]
    pub fn live(&self, tool: Tool, now: i64) -> Option<&UsedUp> {
        self.used_up
            .iter()
            .filter(|r| r.tool == tool && r.live(now))
            .max_by_key(|r| r.until())
    }

    /// The desk's own live record.
    #[must_use]
    pub fn own(&self, session_id: &str, now: i64) -> Option<&UsedUp> {
        self.used_up
            .iter()
            .find(|r| r.session_id == session_id && r.live(now))
    }

    /// How the newest handover of `session_id` for the limit `record` describes went.
    #[must_use]
    pub fn before(&self, session_id: &str, record: &UsedUp) -> Option<&Outcome> {
        self.done
            .iter()
            .filter(|d| {
                d.from_session == session_id
                    && d.from == record.tool
                    && record.same_limit(d.reset_at, d.seen_at)
            })
            .max_by_key(|d| d.at)
            .map(|d| &d.outcome)
    }

    /// "h-1", "h-2", and so on.
    pub fn next_id(&mut self) -> String {
        self.next_id = self.next_id.max(1);
        let id = format!("h-{}", self.next_id);
        self.next_id += 1;
        self.dirty = true;
        id
    }

    /// A handover that failed with `reason` before its old session had ended: once `ended`
    /// is true for that session, the session did end after all, so the failure now says so
    /// with `now_reason`.
    pub fn ended_late(&mut self, reason: &str, now_reason: &str, ended: impl Fn(&str) -> bool) {
        for d in &mut self.done {
            if let Outcome::Failed {
                reason: r,
                ended: gone @ false,
            } = &mut d.outcome
                && r == reason
                && ended(&d.from_session)
            {
                *gone = true;
                *r = now_reason.to_string();
                self.dirty = true;
            }
        }
    }

    /// Never hand out an id below `n`, so an id never meets a brief already on disk.
    pub fn at_least(&mut self, n: u64) {
        if self.next_id < n {
            self.next_id = n;
            self.dirty = true;
        }
    }

    pub fn finished(&mut self, done: Done) {
        self.done.push(done);
        self.dirty = true;
    }

    /// Set the session of the started handover `id`.
    pub fn bound(&mut self, id: &str, session_id: &str) {
        for d in self.done.iter_mut().filter(|d| d.id == id) {
            if let Outcome::Started { session } = &mut d.outcome
                && session.as_deref() != Some(session_id)
            {
                *session = Some(session_id.to_string());
                self.dirty = true;
            }
        }
    }

    /// Drop records that are no longer live, handovers older than 7 days, and all but the newest 200.
    pub fn prune(&mut self, now: i64) {
        let (used, done) = (self.used_up.len(), self.done.len());
        self.used_up.retain(|r| r.live(now));
        self.done.retain(|d| now - d.at <= KEEP_SECONDS);
        if self.done.len() > KEEP_COUNT {
            self.done.sort_by_key(|d| d.at);
            let excess = self.done.len() - KEEP_COUNT;
            self.done.drain(..excess);
        }
        self.dirty |= self.used_up.len() != used || self.done.len() != done;
    }

    #[must_use]
    pub fn used_up(&self) -> &[UsedUp] {
        &self.used_up
    }

    #[must_use]
    pub fn done(&self) -> &[Done] {
        &self.done
    }

    /// Whether anything changed since the last `saved`.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// The file now holds what the book holds.
    pub fn saved(&mut self) {
        self.dirty = false;
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileData {
    version: u32,
    used_up: Vec<UsedUp>,
    done: Vec<Done>,
    next_id: u64,
}

pub struct HandoverFile {
    path: PathBuf,
}

impl HandoverFile {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Never fails. No file: empty. A file that does not read or parse, or has another version, is
    /// moved aside to `handover.invalid.json` and the book starts empty. Records past `now` are dropped.
    #[must_use]
    pub fn load(&self, now: i64) -> Book {
        let Ok(text) = fs::read_to_string(&self.path) else {
            if self.path.exists() {
                self.move_aside();
            }
            return Book::default();
        };
        match serde_json::from_str::<FileData>(&text) {
            Ok(data) if data.version == FILE_VERSION => {
                let mut book = Book {
                    used_up: data.used_up,
                    done: data.done,
                    next_id: data.next_id,
                    dirty: false,
                };
                let held = book.used_up.len();
                book.used_up.retain(|r| r.live(now));
                book.dirty = book.used_up.len() != held;
                book
            }
            _ => {
                self.move_aside();
                Book::default()
            }
        }
    }

    pub fn save(&self, book: &Book) -> Result<(), String> {
        write_json_atomic(
            &self.path,
            &FileData {
                version: FILE_VERSION,
                used_up: book.used_up.clone(),
                done: book.done.clone(),
                next_id: book.next_id,
            },
        )
    }

    fn move_aside(&self) {
        // Best effort: if even the move fails the book still starts empty and the next save replaces the file.
        let _ = fs::rename(
            &self.path,
            self.path.with_file_name("handover.invalid.json"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handover::Work;
    use crate::handover::decide::{Decision, Facts, decide};
    use crate::handover::view::BlockWhy;

    const NOW: i64 = 1_000_000;
    const HOUR: i64 = 3600;

    fn rec(session: &str, reset: Option<i64>, seen: i64) -> UsedUp {
        UsedUp {
            tool: Tool::ClaudeCode,
            window: Some(Window::FiveHour),
            reset_at: reset,
            seen_at: seen,
            session_id: session.into(),
        }
    }

    fn done(session: &str, id: &str, at: i64, reset: Option<i64>, outcome: Outcome) -> Done {
        Done {
            id: id.into(),
            from_session: session.into(),
            from: Tool::ClaudeCode,
            to: Tool::Codex,
            at,
            window: Some(Window::FiveHour),
            reset_at: reset,
            seen_at: NOW,
            outcome,
        }
    }

    fn started() -> Outcome {
        Outcome::Started { session: None }
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ob-handover-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("dir");
        d
    }

    #[test]
    fn saw_table() {
        let mut book = Book::default();
        assert!(book.saw(rec("a", Some(NOW + HOUR), NOW)));
        // The same limit again: false, and seen_at does not move.
        assert!(!book.saw(rec("a", Some(NOW + HOUR), NOW + 50)));
        assert_eq!(book.used_up()[0].seen_at, NOW);
        // A later reset replaces the desk's record.
        assert!(book.saw(rec("a", Some(NOW + 2 * HOUR), NOW + 60)));
        assert_eq!(book.used_up().len(), 1);
        assert_eq!(book.used_up()[0].reset_at, Some(NOW + 2 * HOUR));
        // Another desk is independent.
        assert!(book.saw(rec("b", Some(NOW + 2 * HOUR), NOW + 60)));
        assert_eq!(book.used_up().len(), 2);
    }

    #[test]
    fn saw_without_a_reset_holds_for_unknown_hold() {
        let mut book = Book::default();
        assert!(book.saw(rec("a", None, NOW)));
        assert!(!book.saw(rec("a", None, NOW + UNKNOWN_HOLD - 1)));
        assert!(book.saw(rec("a", None, NOW + UNKNOWN_HOLD)));
        // A reset on one side only is another limit.
        assert!(book.saw(rec(
            "a",
            Some(NOW + UNKNOWN_HOLD + HOUR),
            NOW + UNKNOWN_HOLD
        )));
    }

    #[test]
    fn cleared_own_and_live() {
        let mut book = Book::default();
        book.saw(rec("a", Some(NOW + HOUR), NOW));
        book.saw(rec("b", Some(NOW + 3 * HOUR), NOW));
        book.saw(rec("c", Some(NOW + 2 * HOUR), NOW));
        book.saw(UsedUp {
            tool: Tool::Codex,
            ..rec("d", Some(NOW + 9 * HOUR), NOW)
        });
        assert_eq!(
            book.live(Tool::ClaudeCode, NOW)
                .map(|r| r.session_id.as_str()),
            Some("b")
        );
        assert_eq!(
            book.live(Tool::Codex, NOW).map(|r| r.session_id.as_str()),
            Some("d")
        );
        assert!(book.own("a", NOW).is_some());
        // Not live at the very second of its reset.
        assert!(book.own("a", NOW + HOUR - 1).is_some());
        assert!(book.own("a", NOW + HOUR).is_none());
        assert!(book.own("zzz", NOW).is_none());
        assert!(book.live(Tool::ClaudeCode, NOW + 3 * HOUR).is_none());
        book.saved();
        book.cleared("b");
        assert!(book.dirty());
        assert_eq!(
            book.live(Tool::ClaudeCode, NOW)
                .map(|r| r.session_id.as_str()),
            Some("c")
        );
        book.saved();
        book.cleared("b");
        assert!(!book.dirty());
    }

    #[test]
    fn before_table() {
        let limit = rec("a", Some(NOW + HOUR), NOW);
        let mut book = Book::default();
        assert_eq!(book.before("a", &limit), None);
        book.finished(done(
            "a",
            "h-1",
            10,
            Some(NOW + HOUR),
            Outcome::Failed {
                reason: "x".into(),
                ended: false,
            },
        ));
        assert!(matches!(
            book.before("a", &limit),
            Some(Outcome::Failed { .. })
        ));
        // The newest wins.
        book.finished(done("a", "h-2", 20, Some(NOW + HOUR), started()));
        assert_eq!(book.before("a", &limit), Some(&started()));
        // Another limit, another desk, another tool: nothing.
        book.finished(done("b", "h-3", 30, Some(NOW + HOUR), started()));
        let other_limit = rec("a", Some(NOW + 2 * HOUR), NOW);
        assert_eq!(book.before("a", &other_limit), None);
        assert_eq!(book.before("c", &limit), None);
        let codex = UsedUp {
            tool: Tool::Codex,
            ..limit
        };
        assert_eq!(book.before("a", &codex), None);
    }

    #[test]
    fn ended_late_rewrites_only_a_matching_failure_of_an_ended_session() {
        let failed = |reason: &str| Outcome::Failed {
            reason: reason.into(),
            ended: false,
        };
        let mut book = Book::default();
        book.finished(done("a", "h-1", 10, None, failed("stuck")));
        book.finished(done("b", "h-2", 11, None, failed("stuck")));
        book.finished(done("a", "h-3", 12, None, failed("other")));
        book.saved();
        book.ended_late("stuck", "gone", |id| id == "a");
        assert!(book.dirty());
        assert_eq!(
            book.done()[0].outcome,
            Outcome::Failed {
                reason: "gone".into(),
                ended: true
            }
        );
        assert_eq!(book.done()[1].outcome, failed("stuck"));
        assert_eq!(book.done()[2].outcome, failed("other"));
        book.saved();
        book.ended_late("stuck", "gone", |_| false);
        assert!(!book.dirty());
    }

    #[test]
    fn next_id_and_bound() {
        let mut book = Book::default();
        assert_eq!(book.next_id(), "h-1");
        assert_eq!(book.next_id(), "h-2");
        book.finished(done("a", "h-1", 10, None, started()));
        book.finished(done(
            "a",
            "h-2",
            11,
            None,
            Outcome::Failed {
                reason: "x".into(),
                ended: true,
            },
        ));
        book.saved();
        book.bound("h-1", "n");
        assert!(book.dirty());
        assert_eq!(
            book.done()[0].outcome,
            Outcome::Started {
                session: Some("n".into())
            }
        );
        book.saved();
        book.bound("h-1", "n");
        assert!(!book.dirty());
        // A failure is never bound, and an unknown id does nothing.
        book.bound("h-2", "n");
        book.bound("h-9", "n");
        assert!(!book.dirty());
    }

    #[test]
    fn prune_table() {
        let mut book = Book::default();
        book.saw(rec("a", Some(NOW + HOUR), NOW));
        book.saw(rec("b", Some(NOW - 1), NOW - HOUR));
        book.finished(done("a", "h-1", NOW - 8 * 24 * HOUR, None, started()));
        book.finished(done("a", "h-2", NOW - 6 * 24 * HOUR, None, started()));
        book.prune(NOW);
        assert_eq!(book.used_up().len(), 1);
        assert_eq!(book.done().len(), 1);
        assert_eq!(book.done()[0].id, "h-2");

        let mut many = Book::default();
        for n in 0..250 {
            many.finished(done("a", &format!("h-{n}"), NOW - 250 + n, None, started()));
        }
        many.saved();
        many.prune(NOW);
        assert!(many.dirty());
        assert_eq!(many.done().len(), 200);
        assert_eq!(many.done()[0].id, "h-50");
        many.saved();
        many.prune(NOW);
        assert!(!many.dirty());
    }

    #[test]
    fn dirty_and_saved() {
        let mut book = Book::default();
        assert!(!book.dirty());
        book.saw(rec("a", Some(NOW + HOUR), NOW));
        assert!(book.dirty());
        book.saved();
        assert!(!book.dirty());
        assert!(!book.saw(rec("a", Some(NOW + HOUR), NOW)));
        assert!(!book.dirty());
        let _ = book.next_id();
        assert!(book.dirty());
    }

    #[test]
    fn file_none_gives_an_empty_book() {
        let d = dir("none");
        let book = HandoverFile::new(d.join("handover.json")).load(NOW);
        assert_eq!(book, Book::default());
    }

    #[test]
    fn file_round_trip_is_equal() {
        let d = dir("rt");
        let file = HandoverFile::new(d.join("handover.json"));
        let mut book = Book::default();
        book.saw(rec("a", Some(NOW + HOUR), NOW));
        book.saw(rec("b", None, NOW));
        let id = book.next_id();
        book.finished(done("a", &id, NOW, Some(NOW + HOUR), started()));
        file.save(&book).expect("save");
        let mut loaded = file.load(NOW);
        book.saved();
        assert_eq!(loaded, book);
        assert_eq!(loaded.next_id(), "h-2");
        let text = fs::read_to_string(d.join("handover.json")).expect("read");
        assert!(text.contains(r#""version": 1"#));
        assert!(text.contains(r#""nextId": 2"#));
    }

    #[test]
    fn a_bad_file_is_moved_aside() {
        let cases = [
            ("garbage", "not json".to_string()),
            (
                "version",
                r#"{"version":2,"usedUp":[],"done":[],"nextId":1}"#.to_string(),
            ),
            (
                "unknown",
                r#"{"version":1,"usedUp":[],"done":[],"nextId":1,"extra":true}"#.to_string(),
            ),
        ];
        for (name, text) in cases {
            let d = dir(name);
            fs::write(d.join("handover.json"), &text).expect("write");
            let book = HandoverFile::new(d.join("handover.json")).load(NOW);
            assert_eq!(book, Book::default(), "{name}");
            assert!(!d.join("handover.json").exists(), "{name}");
            assert_eq!(
                fs::read_to_string(d.join("handover.invalid.json")).expect("aside"),
                text,
                "{name}"
            );
        }
    }

    #[test]
    fn restart_keeps_the_record_and_the_started_handover() {
        let d = dir("restart");
        let file = HandoverFile::new(d.join("handover.json"));
        let limit = rec("a", Some(NOW + HOUR), NOW);
        let mut book = Book::default();
        book.saw(limit.clone());
        book.finished(done("a", "h-1", NOW + 5, Some(NOW + HOUR), started()));
        file.save(&book).expect("save");

        let loaded = file.load(NOW + 10);
        let live = loaded.live(Tool::ClaudeCode, NOW + 10).expect("still live");
        assert_eq!(live, &limit);
        let before = loaded.before("a", live);
        assert_eq!(before, Some(&started()));
        let work = Work::Clear;
        let facts = Facts {
            ours: true,
            used_up: true,
            own: true,
            fresh: true,
            automatic: true,
            chosen: true,
            receiver_used_up: false,
            before,
            work: &work,
        };
        assert_eq!(decide(&facts), Decision::Stay(BlockWhy::AlreadyHandedOver));

        let later = file.load(NOW + HOUR);
        assert!(later.used_up().is_empty());
        assert_eq!(later.done().len(), 1);
    }
}
