//! What a scan of a live Codex root hands the reconcile (M2b spec 5.2, 10.3): each worker's
//! name and evidence by thread id, and the workers the disk knows. Synthetic rollouts only,
//! under a temp home, in the shapes of G1 and G2 (`session_meta`).

use super::pets::ProcessRow;
use super::*;
use std::fs;
use std::time::Duration;

const ROOT: u32 = 1;
const ADA: u32 = 2;
const LIN: u32 = 3;

fn uuid(n: u32) -> String {
    format!("0199c0de-0000-4000-8000-{n:012}")
}

/// 2026-09-29T10:00:00Z.
fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_hours(497_410)
}

struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ob-kids-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".codex/sessions/2026/09/29")).expect("sessions");
        Self(dir)
    }

    fn write(&self, n: u32, lines: &[String]) {
        let path = self.0.join(format!(
            ".codex/sessions/2026/09/29/rollout-2026-09-29T10-00-00-{}.jsonl",
            uuid(n)
        ));
        fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write");
        fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(now() - Duration::from_secs(1)))
            .expect("mtime");
    }

    /// A root with the workers Ada (a nickname) and Lin (only an agent path).
    fn team(&self) -> Self {
        let cwd = self.0.join("app");
        let root = format!(
            r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{0}","session_id":"{0}","cwd":"{1}","source":"cli"}}}}"#,
            uuid(ROOT),
            cwd.display()
        );
        self.write(ROOT, &[root, event("task_started", "r1")]);
        let worker = |n: u32, nickname: &str| {
            format!(
                r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{}","cwd":"{}","source":{{"subagent":{{"thread_spawn":{{"parent_thread_id":"{}","depth":1,{nickname}"agent_path":"/root/scout-{n}"}}}}}}}}}}"#,
                uuid(n),
                cwd.display(),
                uuid(ROOT)
            )
        };
        self.write(
            ADA,
            &[
                worker(ADA, r#""agent_nickname":"Ada","#),
                event("task_started", "a1"),
                event("task_complete", "a1"),
            ],
        );
        self.write(LIN, &[worker(LIN, ""), event("task_started", "l1")]);
        Self(self.0.clone())
    }
}

fn event(kind: &str, turn: &str) -> String {
    format!(
        r#"{{"timestamp":"t","type":"event_msg","payload":{{"type":"{kind}","turn_id":"{turn}"}}}}"#
    )
}

fn ended(turn: &str) -> super::codex::turns::Evidence {
    super::codex::turns::Evidence {
        turn_id: Some(turn.to_string()),
        ended_turns: vec![turn.to_string()],
        done_calls: Vec::new(),
    }
}

#[test]
fn a_scan_of_a_live_root_hands_on_each_workers_name_and_evidence() {
    let home = Home::new("scan").team();
    let mut discovery = Discovery::new(home.0.clone());
    discovery.ignore_running_codex();
    let found = discovery.scan(now(), &[uuid(ROOT)]);

    let names: BTreeMap<String, String> = [(ADA, "Ada"), (LIN, "scout-3")]
        .into_iter()
        .map(|(n, name)| (helper_key(&uuid(ROOT), &uuid(n)), name.to_string()))
        .collect();
    assert_eq!(found.helper_names, names, "names by helper key");
    assert_eq!(
        found.codex_gone,
        BTreeMap::from([(uuid(ROOT), BTreeSet::from([uuid(ADA)]))]),
        "the root's workers the disk proves over"
    );
    assert_eq!(
        found.codex.get(&uuid(ADA)),
        Some(&ended("a1")),
        "Ada's evidence"
    );
    let lin = found.codex.get(&uuid(LIN)).expect("Lin's evidence");
    assert_eq!(lin.turn_id.as_deref(), Some("l1"), "Lin is in a turn");
    assert!(lin.ended_turns.is_empty(), "Lin has ended none");
    let root = found.codex.get(&uuid(ROOT)).expect("the root's evidence");
    assert_eq!(
        root.turn_id.as_deref(),
        Some("r1"),
        "the root beside its workers"
    );
    fs::remove_dir_all(&home.0).expect("cleanup");
}

fn row(pid: i32, parent: i32, minutes: u64, command: &str) -> ProcessRow {
    ProcessRow {
        pid,
        parent,
        age: Duration::from_secs(minutes * 60),
        command: command.to_string(),
    }
}

#[test]
fn a_shell_of_a_session_with_a_worker_is_a_pet_and_never_a_helper() {
    let home = Home::new("pets").team();
    let mut source = codex::CodexSource::new(home.0.clone());
    let mut finds = source.scan_with(now(), Vec::new(), &[uuid(ROOT)]);
    let mut find = finds.pop().expect("the root");
    find.live_pid = Some(200);
    let table = [row(200, 1, 60, "codex"), row(201, 200, 5, "zsh")];
    let input = from_codex(find, &table).expect("a session");
    assert_eq!(input.pets.len(), 1, "the shell is a pet");
    assert!(
        input.pets.iter().all(|p| p.id == "pet-201"),
        "and nothing else"
    );
    let helpers: Vec<&str> = input.helpers.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(
        helpers,
        [uuid(ADA), uuid(LIN)],
        "the workers are the helpers"
    );
    fs::remove_dir_all(&home.0).expect("cleanup");
}
