use super::*;
use crate::connectors::fake::{FakeConnector, issue, listing};
use crate::model::Party;
use std::sync::atomic::{AtomicUsize, Ordering};

const EVERY: Duration = Duration::from_secs(60);
const ROOT: &str = "/repo/app";
const NOW: &str = "2026-11-12T10:15:00Z";
const LATER: &str = "2026-11-12T10:16:00Z";

fn link() -> Link {
    Link::Linear {
        team_id: "team-1".to_string(),
        team_key: "ENG".to_string(),
        team_name: "Engineering".to_string(),
        project: None,
        label: None,
    }
}

fn floor(project_id: &str, root: &str) -> LinkedFloor {
    LinkedFloor {
        project_id: project_id.to_string(),
        root: root.to_string(),
        links: vec![link()],
    }
}

struct Rig {
    fake: Arc<FakeConnector>,
    hub: Arc<Hub>,
    changes: Arc<AtomicUsize>,
}

impl Rig {
    fn new() -> Self {
        let fake = Arc::new(FakeConnector::new(EVERY));
        let changes = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&changes);
        let connector: Arc<dyn Connector> = fake.clone();
        let hub = Arc::new(Hub::new(
            vec![connector],
            Box::new(move || {
                counted.fetch_add(1, Ordering::SeqCst);
            }),
        ));
        Self { fake, hub, changes }
    }

    fn changes(&self) -> usize {
        self.changes.load(Ordering::SeqCst)
    }

    fn poll(&self, now: &str) -> Outcome {
        self.hub.poll(&[floor("p-1", ROOT)], now)
    }

    fn todos(&self) -> Vec<TodoItem> {
        self.hub.todos("p-1", ROOT)
    }

    /// One good pass with issues 1 (human) and 2 (agent), then the counter reset.
    fn filled() -> Self {
        let rig = Self::new();
        rig.fake.push_list(Ok(listing(vec![
            issue(1, Party::Human),
            issue(2, Party::Ai),
        ])));
        assert_eq!(rig.poll(NOW), Outcome::Ok);
        rig.changes.store(0, Ordering::SeqCst);
        rig
    }

    fn connection(&self) -> Connection {
        self.hub.connection("p-1", ROOT, &link())
    }
}

fn done_of(todos: &[TodoItem], id: &str) -> Option<bool> {
    todos.iter().find(|t| t.id == id).map(|t| t.done)
}

const ID_1: &str = "p-1|linear|issue-1";
const ID_2: &str = "p-1|linear|issue-2";

#[test]
fn a_pass_fills_a_board() {
    let rig = Rig::new();
    rig.fake.push_list(Ok(listing(vec![
        issue(1, Party::Human),
        issue(2, Party::Ai),
    ])));
    assert_eq!(rig.poll(NOW), Outcome::Ok);
    assert_eq!(rig.changes(), 1);
    assert_eq!(rig.fake.listed(), vec![link()]);
    let todos = rig.todos();
    assert_eq!(todos.len(), 2);
    assert_eq!(todos[0].id, ID_1);
    assert_eq!(todos[0].project_id, "p-1");
    assert_eq!(todos[0].text, "Issue 1");
    assert!(!todos[0].done);
    assert_eq!(todos[0].assignee, Party::Human);
    assert_eq!(todos[1].assignee, Party::Ai);
    assert_eq!(
        todos[0].source,
        TodoSource::Linear {
            identifier: "ENG-1".to_string(),
            url: "https://linear.app/example/issue/ENG-1".to_string(),
        }
    );
    let connection = rig.connection();
    assert_eq!(connection.fetched_at.as_deref(), Some(NOW));
    assert_eq!(connection.error, None);
    assert!(!connection.needs_key);
    assert!(rig.hub.todos("p-2", "/repo/other").is_empty());
}

#[test]
fn a_link_never_checked_has_no_time_and_no_error() {
    let rig = Rig::new();
    let connection = rig.connection();
    assert_eq!(connection.project_id, "p-1");
    assert_eq!(connection.link, link());
    assert_eq!(connection.fetched_at, None);
    assert_eq!(connection.error, None);
    assert!(!connection.needs_key);
    assert!(!connection.truncated);
}

#[test]
fn offline_keeps_the_stale_list() {
    let rig = Rig::filled();
    rig.fake
        .push_list(Err(ConnectorError::Unreachable("offline".to_string())));
    assert_eq!(rig.poll(LATER), Outcome::Failed);
    assert_eq!(rig.todos().len(), 2);
    let connection = rig.connection();
    assert_eq!(connection.fetched_at.as_deref(), Some(NOW));
    assert_eq!(connection.error.as_deref(), Some("offline"));
    assert!(!connection.needs_key);
    assert_eq!(rig.changes(), 1);
}

#[test]
fn no_key_empties_the_board() {
    let rig = Rig::filled();
    rig.fake
        .push_list(Err(ConnectorError::NoKey("no key".to_string())));
    assert_eq!(rig.poll(LATER), Outcome::Ok);
    assert!(rig.todos().is_empty());
    let connection = rig.connection();
    assert_eq!(connection.fetched_at, None);
    assert_eq!(connection.error.as_deref(), Some("no key"));
    assert!(connection.needs_key);
}

#[test]
fn a_refusal_keeps_the_list_and_does_not_slow_the_poller() {
    let rig = Rig::filled();
    rig.fake
        .push_list(Err(ConnectorError::Refused("Linear said: no".to_string())));
    assert_eq!(rig.poll(LATER), Outcome::Ok);
    assert_eq!(rig.todos().len(), 2);
    let connection = rig.connection();
    assert_eq!(connection.fetched_at.as_deref(), Some(NOW));
    assert_eq!(connection.error.as_deref(), Some("Linear said: no"));
}

#[test]
fn a_rate_limit_ends_the_pass() {
    for (asked, waited) in [
        (Some(Duration::from_secs(90)), Duration::from_secs(90)),
        (None, EVERY),
    ] {
        let rig = Rig::new();
        rig.fake.push_list(Err(ConnectorError::RateLimited(asked)));
        rig.fake.push_list(Ok(listing(vec![issue(1, Party::Ai)])));
        let floors = [floor("p-1", ROOT), floor("p-2", "/repo/other")];
        assert_eq!(rig.hub.poll(&floors, NOW), Outcome::RetryAfter(waited));
        assert_eq!(rig.fake.listed().len(), 1);
        assert_eq!(
            rig.connection().error.as_deref(),
            Some("Linear asked the app to slow down")
        );
        assert!(rig.hub.todos("p-2", "/repo/other").is_empty());
    }
}

#[test]
fn a_floor_no_longer_linked_loses_its_board() {
    let rig = Rig::filled();
    assert_eq!(rig.hub.poll(&[], LATER), Outcome::Ok);
    assert!(rig.todos().is_empty());
    assert_eq!(rig.connection().fetched_at, None);
    assert_eq!(rig.changes(), 1);
}

#[test]
fn no_linked_floor_asks_nothing() {
    let rig = Rig::new();
    assert_eq!(rig.hub.poll(&[], NOW), Outcome::Ok);
    assert!(rig.fake.listed().is_empty());
    assert_eq!(rig.changes(), 0);
}

#[test]
fn a_pass_that_changes_nothing_does_not_push() {
    let rig = Rig::filled();
    assert_eq!(rig.poll(NOW), Outcome::Ok);
    assert_eq!(rig.fake.listed().len(), 2);
    assert_eq!(rig.changes(), 0);
}

#[test]
fn a_tick_shows_at_once_and_stays_done() {
    let rig = Rig::filled();
    let seen = Arc::new(Mutex::new(None));
    let (hub, seen_in_tick) = (Arc::clone(&rig.hub), Arc::clone(&seen));
    rig.fake.on_tick(move || {
        *lock(&seen_in_tick) = done_of(&hub.todos("p-1", ROOT), ID_1);
    });
    assert_eq!(rig.hub.tick(ROOT, &link(), "issue-1"), Ok(()));
    assert_eq!(*lock(&seen), Some(true));
    assert_eq!(rig.fake.ticked(), vec![(link(), "issue-1".to_string())]);
    assert_eq!(done_of(&rig.todos(), ID_1), Some(true));
    assert_eq!(rig.todos().len(), 2);
    assert_eq!(rig.changes(), 2);
}

#[test]
fn a_refused_tick_rolls_back() {
    let rig = Rig::filled();
    rig.fake
        .set_tick(Err(ConnectorError::Refused("Linear said: no".to_string())));
    assert_eq!(
        rig.hub.tick(ROOT, &link(), "issue-1"),
        Err("Linear said: no".to_string())
    );
    assert_eq!(done_of(&rig.todos(), ID_1), Some(false));
    assert_eq!(rig.changes(), 2);
}

#[test]
fn a_listing_begun_before_the_tick_does_not_untick_it() {
    let rig = Rig::filled();
    let hub = Arc::clone(&rig.hub);
    rig.fake.on_list(move || {
        assert_eq!(hub.tick(ROOT, &link(), "issue-1"), Ok(()));
    });
    rig.poll(LATER);
    let todos = rig.todos();
    assert_eq!(todos.iter().filter(|t| t.id == ID_1).count(), 1);
    assert_eq!(done_of(&todos, ID_1), Some(true));
    assert_eq!(done_of(&todos, ID_2), Some(false));
}

#[test]
fn the_next_listing_is_the_truth_after_a_tick() {
    let rig = Rig::filled();
    rig.hub.tick(ROOT, &link(), "issue-1").expect("tick");
    rig.fake.push_list(Ok(listing(vec![issue(2, Party::Ai)])));
    rig.fake.push_list(Ok(listing(vec![
        issue(1, Party::Human),
        issue(2, Party::Ai),
    ])));
    rig.poll(LATER);
    assert_eq!(done_of(&rig.todos(), ID_1), None);
    rig.poll(LATER);
    assert_eq!(done_of(&rig.todos(), ID_1), Some(false));
}

#[test]
fn a_second_tick_and_a_gone_item_are_refused() {
    let rig = Rig::filled();
    let hub = Arc::clone(&rig.hub);
    let second = Arc::new(Mutex::new(None));
    let second_in_tick = Arc::clone(&second);
    rig.fake.on_tick(move || {
        *lock(&second_in_tick) = Some(hub.tick(ROOT, &link(), "issue-1"));
    });
    assert_eq!(rig.hub.tick(ROOT, &link(), "issue-1"), Ok(()));
    assert_eq!(
        *lock(&second),
        Some(Err("that item is already being marked done".to_string()))
    );
    assert_eq!(rig.fake.ticked().len(), 1);
    assert_eq!(
        rig.hub.tick(ROOT, &link(), "issue-9"),
        Err(GONE.to_string())
    );
    assert_eq!(
        rig.hub.tick("/repo/other", &link(), "issue-1"),
        Err(GONE.to_string())
    );
}

#[test]
fn a_kind_with_no_connector_is_not_available() {
    let hub = Hub::new(Vec::new(), Box::new(|| {}));
    assert_eq!(
        hub.tick(ROOT, &link(), "issue-1"),
        Err("that connector is not available".to_string())
    );
    assert_eq!(hub.poll(&[floor("p-1", ROOT)], NOW), Outcome::Ok);
}

#[test]
fn urls_forget_and_recheck() {
    let rig = Rig::filled();
    rig.hub.tick(ROOT, &link(), "issue-1").expect("tick");
    let kind = ConnectorKind::Linear;
    assert_eq!(
        rig.hub.url(ROOT, kind, "issue-1").as_deref(),
        Some("https://linear.app/example/issue/ENG-1")
    );
    assert_eq!(
        rig.hub.url(ROOT, kind, "issue-2").as_deref(),
        Some("https://linear.app/example/issue/ENG-2")
    );
    assert_eq!(rig.hub.url(ROOT, kind, "issue-9"), None);
    rig.hub.forget(ROOT, kind);
    assert!(rig.todos().is_empty());
    assert_eq!(rig.hub.url(ROOT, kind, "issue-2"), None);
    rig.hub.recheck();
    assert_eq!(rig.fake.rechecks(), 1);
}

#[test]
fn start_runs_a_pass_at_once_and_a_wake_runs_another() {
    let rig = Rig::new();
    rig.hub.wake();
    start(&rig.hub, || vec![floor("p-1", ROOT)]).expect("start");
    let waited = |count: usize| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while rig.fake.listed().len() < count && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        rig.fake.listed().len() >= count
    };
    assert!(waited(1));
    rig.hub.wake();
    assert!(waited(2));
    assert!(start(&rig.hub, Vec::new).is_err());
}
