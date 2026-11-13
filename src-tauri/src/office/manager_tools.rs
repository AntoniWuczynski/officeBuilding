//! The building's tools for a floor's manager: `impl Tools for Office`. Every call is bound to
//! the floor its connection authenticated for, and nothing here reaches another floor.

use super::deliver::DeskNow;
use super::{Inner, Office, last_lines, now_iso};
use crate::delivery::gate::{Hold, Route};
use crate::manager::tools::{Call, ReadDesk, Tools};
use crate::model::{Decision, Party, SessionState, TodoSource, ToolKind};
use crate::project_files;
use crate::structured::{Entry, ToolStatus};
use serde::Serialize;

/// The most items `list_todos` hands back, and the most characters of each one's text.
const TODOS_MAX: usize = 200;
const TODO_TEXT_MAX: usize = 300;
/// The newest decisions `list_todos` hands back.
const DECISIONS_MAX: usize = 10;
/// A desk id longer than this is no desk.
const DESK_ID_MAX: usize = 200;
/// The lines `read_desk` reads by default and at most, and the most characters it hands back.
const READ_LINES: u32 = 40;
const READ_LINES_MAX: u32 = 200;
const READ_TEXT_MAX: usize = 8000;

const HAND_STARTED: &str =
    "This desk was started outside the building, so what it says cannot be read from here.";

const NO_SUCH_DESK: &str = "there is no such desk on your floor";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TodoList {
    todos: Vec<Todo>,
    open_calls: Vec<OpenCall>,
    decisions: Vec<Ruling>,
}

#[derive(Serialize)]
struct Todo {
    id: String,
    text: String,
    done: bool,
    assignee: Party,
    source: String,
}

#[derive(Serialize)]
struct OpenCall {
    id: String,
    prompt: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ruling {
    title: String,
    detail: String,
    decided_at: String,
    by: Party,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeskList {
    floor: String,
    paused: bool,
    hiring: Hiring,
    desks: Vec<DeskLine>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Hiring {
    allowed: bool,
    max_desks: u32,
    hired_now: u32,
    spend_ceiling_usd: u32,
    spent_usd: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeskLine {
    id: String,
    title: String,
    tool: ToolKind,
    model: Option<String>,
    effort: Option<String>,
    state: SessionState,
    last_activity_at: String,
    spend_usd: f64,
    /// None: a note would be handed over now.
    held: Option<Hold>,
    /// Notes waiting for it.
    waiting: usize,
    hired_by_you: bool,
}

#[derive(Serialize)]
struct DeskRead {
    desk: String,
    title: String,
    state: SessionState,
    source: &'static str,
    text: String,
}

impl Tools for Office {
    fn call(&self, floor: &str, call: Call) -> Result<String, String> {
        let stamped = {
            let mut inner = self.lock();
            if !inner.found.roots.contains_key(floor) {
                return Err("your floor is no longer in the building".to_string());
            }
            let now = now_iso();
            inner
                .seats
                .get_mut(floor)
                .is_some_and(|seat| seat.last_call_at.replace(now.clone()) != Some(now))
        };
        let reads = matches!(call, Call::ListTodos | Call::ListDesks | Call::ReadDesk(_));
        // The tools that act take the lock themselves, and never across a hand-over, a hire
        // or a wait.
        let answer = match call {
            Call::ListTodos => self.list_todos(&self.lock(), floor),
            Call::ListDesks => self.list_desks(&self.lock(), floor),
            Call::ReadDesk(args) => self.read_desk(&self.lock(), floor, &args),
            Call::SendToDesk(args) => self.send_to_desk(floor, &args),
            Call::WaitForDesk(args) => self.wait_for_desk(floor, &args),
            Call::HireDesk(args) => self.hire_desk(floor, &args),
            Call::Escalate(args) => self.escalate(floor, &args),
        };
        // Outside the lock: the callback reads a snapshot.
        if (stamped || !reads)
            && let Ok(host) = &self.manager
        {
            (host.changed)();
        }
        answer
    }
}

pub(super) fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value)
        .map_err(|e| format!("the building could not write its answer ({e})"))
}

/// The last `max` characters of `text`.
fn tail(text: &str, max: usize) -> String {
    let skip = text.chars().count().saturating_sub(max);
    text.chars().skip(skip).collect()
}

/// One transcript entry as a line `read_desk` shows. Thoughts are left out.
fn entry_line(entry: &Entry) -> Option<String> {
    match entry {
        Entry::User { text, .. } => Some(format!("you: {text}")),
        Entry::Agent { text, .. } => Some(format!("agent: {text}")),
        Entry::Thought { .. } => None,
        Entry::Tool { title, status, .. } => {
            let status = match status {
                ToolStatus::Pending => "pending",
                ToolStatus::Running => "running",
                ToolStatus::Done => "done",
                ToolStatus::Failed => "failed",
            };
            Some(format!("tool: {title} ({status})"))
        }
        Entry::Notice { text, .. } => Some(format!("notice: {text}")),
    }
}

/// The desk `id` names on `floor`, as a tool may use it. A desk on another floor, a manager, a
/// QA reviewer and a desk that does not exist all get the same answer.
pub(super) fn floor_desk(
    office: &Office,
    inner: &Inner,
    floor: &str,
    id: &str,
) -> Result<DeskNow, String> {
    if id.chars().count() > DESK_ID_MAX {
        return Err(NO_SUCH_DESK.to_string());
    }
    let staff = Office::staff_sessions(inner);
    office
        .desk_now(inner, id)
        .filter(|desk| {
            desk.project_id == floor && !staff.iter().any(|s| *s == desk.session_id || s == id)
        })
        .ok_or_else(|| NO_SUCH_DESK.to_string())
}

impl Office {
    /// The desks on the floor (not its staff), with whether a note would reach each now.
    fn list_desks(&self, inner: &Inner, floor: &str) -> Result<String, String> {
        let (projects, sessions, _) = super::with_structured(&inner.found, &self.structured);
        let staff = Self::staff_sessions(inner);
        let hired = Self::hired_sessions(inner, floor);
        let record = Self::manager_record(inner, floor);
        let caps = self.manager_caps(inner, floor);
        let desks = sessions
            .into_iter()
            .filter(|s| s.project_id == floor && !staff.contains(&s.id))
            .map(|s| {
                // A desk no note can find has finished: its terminal has closed.
                let held = self
                    .desk_now(inner, &s.id)
                    .map_or(Some(Hold::Finished), |desk| {
                        Self::hold_for(inner, floor, &desk)
                    });
                DeskLine {
                    waiting: inner
                        .mail
                        .outbox
                        .items()
                        .iter()
                        .filter(|item| item.to == s.id)
                        .count(),
                    hired_by_you: hired.contains(&s.id),
                    held,
                    spend_usd: s.spend.usd,
                    id: s.id,
                    title: s.title,
                    tool: s.tool,
                    model: s.model,
                    effort: s.effort,
                    state: s.state,
                    last_activity_at: s.last_activity_at,
                }
            })
            .collect();
        json(&DeskList {
            floor: projects
                .into_iter()
                .find(|p| p.id == floor)
                .map_or_else(|| floor.to_string(), |p| p.name),
            paused: record.paused,
            hiring: Hiring {
                allowed: record.may_hire,
                max_desks: caps.max_desks,
                hired_now: caps.hired_now,
                spend_ceiling_usd: caps.spend_ceiling_usd,
                spent_usd: caps.spent_usd,
            },
            desks,
        })
    }

    /// The last lines a desk printed or said, at most `READ_TEXT_MAX` characters of them.
    fn read_desk(&self, inner: &Inner, floor: &str, args: &ReadDesk) -> Result<String, String> {
        let desk = floor_desk(self, inner, floor, &args.desk)?;
        let lines = args.lines.unwrap_or(READ_LINES).clamp(1, READ_LINES_MAX);
        let lines = usize::try_from(lines).unwrap_or(usize::MAX);
        let (source, text) = match (&desk.desk.route, &desk.terminal) {
            (Route::Structured, _) => {
                let said: Vec<String> = self
                    .structured
                    .backlog(&desk.key)?
                    .transcript
                    .iter()
                    .filter_map(entry_line)
                    .collect();
                let from = said.len().saturating_sub(lines);
                ("conversation", said[from..].join("\n"))
            }
            (_, Some(terminal)) => (
                "terminal",
                last_lines(&self.terminals, terminal, lines)?.join("\n"),
            ),
            (_, None) => ("none", HAND_STARTED.to_string()),
        };
        json(&DeskRead {
            desk: desk.session_id,
            title: desk.title,
            state: desk.desk.state,
            source,
            text: tail(&text, READ_TEXT_MAX),
        })
    }

    /// The floor's TODO items, its open calls and its newest decisions.
    fn list_todos(&self, inner: &Inner, floor: &str) -> Result<String, String> {
        let mut items = Self::own_todos(inner, floor, Party::Human);
        items.extend(Self::own_todos(inner, floor, Party::Ai));
        if let Some(root) = inner.found.roots.get(floor) {
            items.extend(self.connectors.todos(floor, &root.to_string_lossy()));
        }
        let todos = items
            .into_iter()
            .take(TODOS_MAX)
            .map(|t| Todo {
                id: t.id,
                text: project_files::truncate(&t.text, TODO_TEXT_MAX),
                done: t.done,
                assignee: t.assignee,
                source: match t.source {
                    TodoSource::File { file } => file,
                    TodoSource::Linear { identifier, .. } => format!("Linear {identifier}"),
                    TodoSource::App => "the building".to_string(),
                },
            })
            .collect();
        let files = inner.files.get(floor);
        let records = &inner.store.records;
        let open_calls = files
            .into_iter()
            .flat_map(|f| f.open.iter())
            .chain(records.calls.iter().filter(|q| q.project_id == floor))
            .map(|q| OpenCall {
                id: q.id.clone(),
                prompt: q.prompt.clone(),
            })
            .collect();
        let mut decided: Vec<&Decision> = files
            .into_iter()
            .flat_map(|f| f.decided.iter().flatten())
            .chain(records.decisions.iter().filter(|d| d.project_id == floor))
            .collect();
        decided.sort_by(|a, b| b.decided_at.cmp(&a.decided_at));
        let decisions = decided
            .into_iter()
            .take(DECISIONS_MAX)
            .map(|d| Ruling {
                title: d.title.clone(),
                detail: d.detail.clone(),
                decided_at: d.decided_at.clone(),
                by: d.by,
            })
            .collect();
        json(&TodoList {
            todos,
            open_calls,
            decisions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::manager::tests::{bind, manager_office, seat, session as desk, wait_for};
    use super::super::tests::floor_office;
    use super::*;
    use crate::manager::tools::WaitForDesk;
    use crate::manager::wire::ManagerStatus;
    use crate::model::{AnswerVia, ControlMode, Question, SessionState, Spend, ToolKind};
    use portable_pty::CommandBuilder;
    use serde::Deserialize;
    use std::fs;
    use std::sync::atomic::Ordering;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestList {
        todos: Vec<TestTodo>,
        open_calls: Vec<TestCall>,
        decisions: Vec<TestDecision>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestTodo {
        id: String,
        text: String,
        done: bool,
        assignee: Party,
        source: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestCall {
        id: String,
        prompt: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestDecision {
        title: String,
        detail: String,
        decided_at: String,
        by: Party,
    }

    fn decision(id: &str, project_id: &str, at: &str) -> Decision {
        Decision {
            id: id.to_string(),
            project_id: project_id.to_string(),
            title: format!("title {id}"),
            detail: "detail".to_string(),
            decided_at: at.to_string(),
            by: Party::Human,
        }
    }

    fn call(project_id: &str, id: &str) -> Question {
        Question {
            id: id.to_string(),
            project_id: project_id.to_string(),
            session_id: String::new(),
            prompt: format!("MGR-1: {id}"),
            options: Vec::new(),
            allow_other: true,
            answer_via: AnswerVia::Store,
            priority: 0,
            asked_at: "2026-11-12T10:00:00Z".to_string(),
            context: String::new(),
            source_file: None,
        }
    }

    #[test]
    fn list_todos_answers_for_the_callers_floor_only() {
        let (office, dir, mine) = floor_office("tools-todos");
        let other_root = dir.join("home/code/other");
        fs::create_dir_all(&other_root).unwrap();
        let other = office.add_floor(&other_root.to_string_lossy()).unwrap();
        let long = "x".repeat(400);
        fs::write(
            dir.join("home/code/app/TODO.md"),
            format!("# TODO\n- [ ] mine to do\n- [x] mine done\n- [ ] {long}\n"),
        )
        .unwrap();
        fs::write(other_root.join("TODO.md"), "# TODO\n- [ ] theirs\n").unwrap();
        fs::write(
            dir.join("home/code/app/HUMAN_DECISIONS.md"),
            "# Human decisions\n\n## D-1 — Which colour?\n\nPick one.\n\n- **Red**\n- **Blue**\n",
        )
        .unwrap();
        office.refresh();
        {
            let mut inner = office.lock();
            let records = &mut inner.store.records;
            records.calls.push(call(&mine.id, "c-1"));
            records.calls.push(call(&other.id, "c-2"));
            for i in 0..12 {
                records.decisions.push(decision(
                    &format!("d-{i}"),
                    &mine.id,
                    &format!("2026-11-{:02}T10:00:00Z", i + 1),
                ));
            }
            records
                .decisions
                .push(decision("d-other", &other.id, "2026-12-01T10:00:00Z"));
        }

        let text = office.call(&mine.id, Call::ListTodos).unwrap();
        let list: TestList = serde_json::from_str(&text).unwrap();
        let texts: Vec<&str> = list.todos.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts[..2], ["mine to do", "mine done"]);
        assert_eq!(list.todos.len(), 3);
        assert_eq!(list.todos[2].text.chars().count(), TODO_TEXT_MAX);
        assert!(list.todos[1].done && !list.todos[0].done);
        assert!(list.todos.iter().all(|t| t.source == "TODO.md"));
        assert!(list.todos.iter().all(|t| t.assignee == Party::Ai));
        assert!(list.todos.iter().all(|t| t.id.starts_with(&mine.id)));

        let calls: Vec<&str> = list.open_calls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(calls.len(), 2, "the file's call and the store's: {calls:?}");
        assert!(calls.contains(&"c-1") && !calls.contains(&"c-2"));
        assert!(
            list.open_calls
                .iter()
                .any(|c| c.prompt.contains("Which colour?"))
        );

        assert_eq!(list.decisions.len(), DECISIONS_MAX);
        assert_eq!(list.decisions[0].title, "title d-11");
        assert_eq!(list.decisions[0].decided_at, "2026-11-12T10:00:00Z");
        assert_eq!(list.decisions[0].by, Party::Human);
        assert!(list.decisions.iter().all(|d| d.title != "title d-other"));
        assert!(list.decisions.iter().all(|d| d.detail == "detail"));

        let theirs: TestList =
            serde_json::from_str(&office.call(&other.id, Call::ListTodos).unwrap()).unwrap();
        assert_eq!(theirs.todos.len(), 1);
        assert_eq!(theirs.todos[0].text, "theirs");
        assert_eq!(theirs.open_calls.len(), 1);
        assert_eq!(theirs.decisions.len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_floor_that_is_not_standing_is_refused_and_every_tool_is_built() {
        let (office, dir, floor) = floor_office("tools-refuse");
        assert_eq!(
            office.call("p-gone", Call::ListTodos),
            Err("your floor is no longer in the building".to_string())
        );
        assert_eq!(
            office.call(
                &floor.id,
                Call::WaitForDesk(WaitForDesk {
                    desk: "d".to_string(),
                    seconds: None
                })
            ),
            Err(NO_SUCH_DESK.to_string())
        );
        assert!(!include_str!("manager_tools.rs").contains(concat!("has not got ", "that tool")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn floor_desk_gives_one_answer_for_every_desk_that_is_not_on_the_floor() {
        let (office, dir, floor) = floor_office("tools-desk");
        let session = |id: &str, project_id: &str| crate::model::Session {
            id: id.to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: project_id.to_string(),
            title: id.to_string(),
            state: SessionState::Idle,
            control: ControlMode::RaiseWindow,
            model: None,
            effort: None,
            last_activity_at: String::new(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers: Vec::new(),
            pets: Vec::new(),
            worktree: false,
            branch: None,
        };
        {
            let mut inner = office.lock();
            inner.found.sessions.push(session("here", &floor.id));
            inner.found.sessions.push(session("there", "p-other"));
            inner.found.sessions.push(session("reviewer", &floor.id));
            inner
                .owned
                .insert("t-r".to_string(), "reviewer".to_string());
            inner.reviewers.insert("t-r".to_string(), floor.id.clone());
        }
        let inner = office.lock();
        let found = floor_desk(&office, &inner, &floor.id, "here").unwrap();
        assert_eq!(found.session_id, "here");
        for id in ["there", "reviewer", "nobody", &"h".repeat(201)] {
            assert_eq!(
                floor_desk(&office, &inner, &floor.id, id).err(),
                Some(NO_SUCH_DESK.to_string()),
                "{id}"
            );
        }
        drop(inner);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestDesks {
        floor: String,
        paused: bool,
        hiring: TestHiring,
        desks: Vec<TestDesk>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestHiring {
        allowed: bool,
        max_desks: u32,
        hired_now: u32,
        spend_ceiling_usd: u32,
        spent_usd: f64,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestDesk {
        id: String,
        title: String,
        tool: ToolKind,
        model: Option<String>,
        effort: Option<String>,
        state: SessionState,
        last_activity_at: String,
        spend_usd: f64,
        held: Option<Hold>,
        waiting: usize,
        hired_by_you: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestRead {
        desk: String,
        title: String,
        state: SessionState,
        source: String,
        text: String,
    }

    fn read(
        office: &Office,
        floor: &str,
        id: &str,
        lines: Option<u32>,
    ) -> Result<TestRead, String> {
        let call = Call::ReadDesk(ReadDesk {
            desk: id.to_string(),
            lines,
        });
        office
            .call(floor, call)
            .map(|text| serde_json::from_str(&text).unwrap())
    }

    /// A desk on `floor` running `program` with `args` in an app terminal, bound to session `id`.
    fn terminal_desk(
        office: &Office,
        dir: &std::path::Path,
        floor: &str,
        id: &str,
        args: &[&str],
    ) -> String {
        let mut command = CommandBuilder::new(args[0]);
        command.args(&args[1..]);
        let terminal = office
            .terminals
            .spawn(command, &dir.join("home/code/app"))
            .unwrap();
        bind(office, &terminal, id, floor);
        terminal
    }

    #[test]
    fn list_desks_shows_the_floors_desks_and_none_of_its_staff() {
        let (office, dir, floor, registry, _) = manager_office("tools-list");
        let (manager, _) = seat(&office, &dir, &floor, &registry, "/bin/cat");
        bind(&office, &manager, "s-manager", &floor.id);
        terminal_desk(&office, &dir, &floor.id, "s-desk", &["/bin/cat"]);
        let shell = office.open_shell(&floor.id).unwrap();
        {
            let mut inner = office.lock();
            let mut hand = desk("s-hand", &floor.id, ControlMode::RaiseWindow);
            hand.spend.usd = 0.75;
            inner.found.sessions.push(hand);
            inner
                .found
                .sessions
                .push(desk("s-there", "p-other", ControlMode::RaiseWindow));
            inner
                .found
                .sessions
                .push(desk("s-reviewer", &floor.id, ControlMode::Full));
            inner
                .owned
                .insert("t-r".to_string(), "s-reviewer".to_string());
            inner.reviewers.insert("t-r".to_string(), floor.id.clone());
            let note = super::super::deliver::NewNote {
                floor: floor.id.clone(),
                from: "s-manager".to_string(),
                to: "s-hand".to_string(),
                intro: String::new(),
                text: "Take TODO.md.".to_string(),
            };
            Office::post_note(&mut inner, note).unwrap();
        }
        office.set_manager_hiring(&floor.id, true).unwrap();

        let list: TestDesks =
            serde_json::from_str(&office.call(&floor.id, Call::ListDesks).unwrap()).unwrap();
        assert_eq!(list.floor, "app");
        assert!(!list.paused);
        assert!(list.hiring.allowed);
        assert_eq!(
            (
                list.hiring.max_desks,
                list.hiring.hired_now,
                list.hiring.spend_ceiling_usd
            ),
            (3, 0, 20)
        );
        assert!(list.hiring.spent_usd.abs() < f64::EPSILON);
        let mut ids: Vec<&str> = list.desks.iter().map(|d| d.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            vec!["s-desk", "s-hand"],
            "not {} nor the staff",
            shell.id
        );
        let hand = list.desks.iter().find(|d| d.id == "s-hand").unwrap();
        assert_eq!(hand.held, Some(Hold::HandStarted));
        assert_eq!(hand.waiting, 1);
        assert!((hand.spend_usd - 0.75).abs() < f64::EPSILON);
        assert_eq!(
            (hand.title.as_str(), hand.tool, hand.state),
            ("desk s-hand", ToolKind::ClaudeCode, SessionState::Idle)
        );
        assert_eq!(
            (hand.model.as_deref(), hand.effort.as_deref()),
            (None, None)
        );
        assert_eq!(hand.last_activity_at, "2026-11-12T10:00:00Z");
        assert!(!hand.hired_by_you);
        let at_desk = list.desks.iter().find(|d| d.id == "s-desk").unwrap();
        // No hook event has reported it yet.
        assert_eq!(at_desk.held, Some(Hold::NoIdleSignal));
        assert_eq!(at_desk.waiting, 0);

        office.set_manager_paused(&floor.id, true).unwrap();
        let list: TestDesks =
            serde_json::from_str(&office.call(&floor.id, Call::ListDesks).unwrap()).unwrap();
        assert!(list.paused);
        assert!(list.desks.iter().all(|d| d.held == Some(Hold::Paused)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_desk_reads_a_terminals_tail_within_its_limits() {
        let (office, dir, floor, _, _) = manager_office("tools-read");
        let count =
            "i=0; while [ $i -lt 300 ]; do printf '%03d\\n' $i; i=$((i+1)); done; exec /bin/cat";
        let short = terminal_desk(
            &office,
            &dir,
            &floor.id,
            "s-short",
            &["/bin/sh", "-c", count],
        );
        let wide = "i=0; while [ $i -lt 150 ]; do printf '%0100d\\n' $i; i=$((i+1)); done; echo end; exec /bin/cat";
        let long = terminal_desk(&office, &dir, &floor.id, "s-long", &["/bin/sh", "-c", wide]);
        let screen = |t: &str, mark: &str| {
            office
                .terminals
                .backlog(t)
                .is_ok_and(|b| b.data.contains(mark))
        };
        wait_for("the count", || screen(&short, "299"));
        wait_for("the wide lines", || screen(&long, "end"));

        let tail = read(&office, &floor.id, "s-short", Some(3)).unwrap();
        assert_eq!(tail.source, "terminal");
        assert_eq!(tail.text, "297\n298\n299");
        assert_eq!(
            (tail.desk.as_str(), tail.title.as_str()),
            ("s-short", "desk s-short")
        );
        assert_eq!(
            tail.state,
            SessionState::Running,
            "no hook event has said idle"
        );
        let lines = |n: Option<u32>| {
            read(&office, &floor.id, "s-short", n)
                .unwrap()
                .text
                .lines()
                .count()
        };
        assert_eq!(lines(None), 40);
        assert_eq!(lines(Some(0)), 1);
        assert_eq!(lines(Some(1000)), 200);
        let capped = read(&office, &floor.id, "s-long", Some(200)).unwrap().text;
        assert_eq!(capped.chars().count(), READ_TEXT_MAX);
        assert!(capped.ends_with("end"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_desk_gives_one_refusal_and_cannot_read_a_hand_started_desk() {
        let (office, dir, floor, registry, calls) = manager_office("tools-read-refuse");
        let (manager, _) = seat(&office, &dir, &floor, &registry, "/bin/cat");
        bind(&office, &manager, "s-manager", &floor.id);
        {
            let mut inner = office.lock();
            inner
                .found
                .sessions
                .push(desk("s-hand", &floor.id, ControlMode::ReadOnly));
            inner
                .found
                .sessions
                .push(desk("s-there", "p-other", ControlMode::RaiseWindow));
        }
        for id in ["s-there", "s-nobody", "s-manager"] {
            assert_eq!(
                read(&office, &floor.id, id, None).err(),
                Some(NO_SUCH_DESK.to_string()),
                "{id}"
            );
        }
        let hand = read(&office, &floor.id, "s-hand", None).unwrap();
        assert_eq!(
            (hand.source.as_str(), hand.text.as_str()),
            ("none", HAND_STARTED)
        );

        // A call stamps when the manager last used the building's tools, and says so once a second.
        let stamped = office
            .snapshot()
            .managers
            .into_iter()
            .find(|m| m.project_id == floor.id)
            .unwrap()
            .status;
        assert!(matches!(
            stamped,
            ManagerStatus::Hired {
                last_call_at: Some(_),
                ..
            }
        ));
        assert!(calls.load(Ordering::Relaxed) >= 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
