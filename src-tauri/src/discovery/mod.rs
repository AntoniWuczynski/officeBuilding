//! Session discovery: reads each tool's own files and assembles the floors,
//! desks and waiting questions. Claude Code and Codex today.

pub mod claude_code;
pub mod codex;
pub mod pricing;
pub mod rules;

use crate::model::{
    AnswerVia, ControlMode, Project, Question, QuestionOption, Session, SessionState, Spend,
    ToolKind,
};
use rules::Signals;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What discovery currently sees.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Discovered {
    /// Absolute root folder of each floor, by project id.
    pub roots: BTreeMap<String, PathBuf>,
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
    pub questions: Vec<Question>,
}

/// FNV-1a, 32-bit: the same stable hash the frontend uses for looks.
#[must_use]
pub fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// The floor a working directory belongs to: its git root, or itself.
#[must_use]
pub fn project_root(cwd: &Path) -> PathBuf {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return d.to_path_buf();
        }
        dir = d.parent();
    }
    cwd.to_path_buf()
}

#[must_use]
pub fn project_id(root: &Path) -> String {
    format!("p-{:08x}", fnv1a(&root.to_string_lossy()))
}

/// `/Users/me/code/app` → `~/code/app`.
#[must_use]
pub fn display_path(root: &Path, home: &Path) -> String {
    match root.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => root.to_string_lossy().into_owned(),
    }
}

/// Runs every source and assembles the office.
pub struct Discovery {
    home: PathBuf,
    claude: claude_code::ClaudeSource,
    codex: codex::CodexSource,
}

impl Discovery {
    #[must_use]
    pub fn new(home: PathBuf) -> Self {
        Self {
            claude: claude_code::ClaudeSource::new(home.clone()),
            codex: codex::CodexSource::new(home.clone()),
            home,
        }
    }

    pub fn scan(&mut self, now: SystemTime) -> Discovered {
        let inputs = self
            .claude
            .scan(now)
            .into_iter()
            .filter_map(from_claude)
            .chain(self.codex.scan(now).into_iter().filter_map(from_codex))
            .collect();
        assemble(&self.home, inputs, project_root)
    }

    /// Pid of a running session, for focusing its terminal window.
    #[must_use]
    pub fn pid_of(&self, session_id: &str) -> Option<i32> {
        self.claude
            .pid_of(session_id)
            .or_else(|| self.codex.pid_of(session_id))
    }
}

/// A formal question a tool asked, tool-agnostic.
struct FormalQuestionInput {
    /// The part of the question's id unique within the session (e.g. the tool-use id).
    id_suffix: String,
    prompt: String,
    options: Vec<String>,
    asked_at: String,
}

/// One discovered session, reduced to what `assemble` needs, independent of
/// which tool found it.
struct SessionInput {
    session_id: String,
    tool: ToolKind,
    cwd: String,
    title: Option<String>,
    live: bool,
    model: Option<String>,
    effort: Option<String>,
    last_activity: String,
    spend: Spend,
    signals: Signals,
    /// A formal question (e.g. Claude Code's `AskUserQuestion`), when the tool has one.
    /// Otherwise a waiting state falls back to the plain-prose scorer.
    formal_question: Option<FormalQuestionInput>,
}

fn from_claude(f: claude_code::Found) -> Option<SessionInput> {
    let cwd = f
        .transcript
        .cwd
        .clone()
        .or_else(|| f.live.as_ref().map(|l| l.cwd.clone()))?;
    let signals = claude_code::signals(&f.transcript, f.live.as_ref());
    let title = f
        .transcript
        .title
        .clone()
        .or_else(|| f.live.as_ref().and_then(|l| l.name.clone()));
    let last_activity = f.transcript.last_activity.clone().unwrap_or_default();
    let formal_question = match (&f.transcript.question, &f.live) {
        (Some(q), _) => Some(FormalQuestionInput {
            id_suffix: q.tool_use_id.clone(),
            prompt: q.prompt.clone(),
            options: q.options.clone(),
            asked_at: q.asked_at.clone(),
        }),
        // An open question or permission prompt reaches the transcript only once answered.
        (None, Some(live)) if live.waiting => Some(FormalQuestionInput {
            id_suffix: "waiting".to_string(),
            prompt: "Waiting for you in its terminal".to_string(),
            options: Vec::new(),
            asked_at: last_activity.clone(),
        }),
        _ => None,
    };
    Some(SessionInput {
        session_id: f.session_id,
        tool: ToolKind::ClaudeCode,
        cwd,
        title,
        live: f.live.is_some(),
        model: f.transcript.model.clone(),
        effort: f.transcript.effort.clone(),
        last_activity,
        spend: f.transcript.spend,
        signals,
        formal_question,
    })
}

fn from_codex(f: codex::Found) -> Option<SessionInput> {
    let cwd = f.transcript.cwd.clone()?;
    let signals = codex::signals(&f.transcript, f.live_pid);
    Some(SessionInput {
        session_id: f.session_id,
        tool: ToolKind::Codex,
        cwd,
        title: f.title,
        live: f.live_pid.is_some(),
        model: f.transcript.model.clone(),
        effort: f.transcript.effort.clone(),
        last_activity: f.transcript.last_activity.clone().unwrap_or_default(),
        spend: f.transcript.spend,
        signals,
        // Codex has no formal question/approval event in the rollouts inspected on this Mac
        // (checked 2026-09-29): no `approval`- or `input-request`-shaped event exists. A
        // waiting Codex session always falls back to the plain-prose scorer.
        formal_question: None,
    })
}

/// Build floors, desks and questions from every tool's finds. `root_of` maps a
/// working directory to its floor (git root in production, identity in tests).
fn assemble(
    home: &Path,
    inputs: Vec<SessionInput>,
    root_of: impl Fn(&Path) -> PathBuf,
) -> Discovered {
    let mut floors: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut sessions = Vec::new();
    let mut questions = Vec::new();

    for input in inputs {
        let root = root_of(Path::new(&input.cwd));
        // An ended session leaves the office; its floor stays while the session was recent.
        let ids = floors.entry(root.clone()).or_default();
        if !input.live {
            continue;
        }
        ids.push(input.session_id.clone());
        let state = rules::derive_state(&input.signals);

        let mut pending = Vec::new();
        if state == SessionState::WaitingHuman {
            let question = match &input.formal_question {
                Some(q) => Question {
                    id: format!("{}:{}", input.session_id, q.id_suffix),
                    project_id: project_id(&root),
                    session_id: input.session_id.clone(),
                    prompt: q.prompt.clone(),
                    options: q
                        .options
                        .iter()
                        .enumerate()
                        .map(|(i, label)| QuestionOption {
                            id: format!("opt-{i}"),
                            label: label.clone(),
                        })
                        .collect(),
                    allow_other: true,
                    answer_via: AnswerVia::Terminal,
                    priority: 0,
                    asked_at: q.asked_at.clone(),
                    context: String::new(),
                    source_file: None,
                },
                // A plain-prose ask (soft block): no options, the prompt is the asking line.
                None => Question {
                    id: format!("{}:prose", input.session_id),
                    project_id: project_id(&root),
                    session_id: input.session_id.clone(),
                    prompt: rules::score_soft_block(&input.signals.reply_to_human).question,
                    options: Vec::new(),
                    allow_other: true,
                    answer_via: AnswerVia::Terminal,
                    priority: 1,
                    asked_at: input.last_activity.clone(),
                    context: String::new(),
                    source_file: None,
                },
            };
            pending.push(question.id.clone());
            questions.push(question);
        }

        let title = input
            .title
            .clone()
            .unwrap_or_else(|| "Untitled session".to_string());
        sessions.push(Session {
            id: input.session_id,
            tool: input.tool,
            project_id: project_id(&root),
            title,
            state,
            control: ControlMode::RaiseWindow,
            model: input.model,
            effort: input.effort,
            last_activity_at: input.last_activity,
            pending_question_ids: pending,
            spend: input.spend,
        });
    }

    let root_by_id: BTreeMap<String, PathBuf> =
        floors.keys().map(|r| (project_id(r), r.clone())).collect();
    let projects: Vec<Project> = floors
        .into_iter()
        .map(|(root, session_ids)| Project {
            id: project_id(&root),
            name: root.file_name().map_or_else(
                || root.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            ),
            path: display_path(&root, home),
            session_ids,
        })
        .collect();
    // Most urgent first, then oldest ask first.
    questions.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.asked_at.cmp(&b.asked_at))
    });
    Discovered {
        roots: root_by_id,
        projects,
        sessions,
        questions,
    }
}

/// Every `*.jsonl` under `dir`, at any depth.
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().is_some_and(|x| x == "jsonl") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::claude_code::{Found, LiveEntry, parse_transcript};
    use super::*;
    use std::io::Cursor;

    fn found(id: &str, cwd: &str, lines: &[&str], live: bool) -> Found {
        Found {
            session_id: id.to_string(),
            transcript: parse_transcript(Cursor::new(lines.join("\n"))),
            live: live.then(|| LiveEntry {
                pid: 1,
                session_id: id.to_string(),
                cwd: cwd.to_string(),
                busy: false,
                waiting: false,
                name: None,
            }),
        }
    }

    fn claude_inputs(founds: Vec<Found>) -> Vec<SessionInput> {
        founds.into_iter().filter_map(from_claude).collect()
    }

    const ASK: &str = r#"{"type":"assistant","cwd":"/home/me/code/app","timestamp":"2026-09-29T10:01:00Z","message":{"id":"m","content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion","input":{"questions":[{"question":"Which?","options":[{"label":"A"},{"label":"B"}]}]}}]}}"#;
    const PROSE: &str = r#"{"type":"assistant","cwd":"/home/me/code/app","timestamp":"2026-09-29T10:00:00Z","message":{"id":"m","content":[{"type":"text","text":"Fixed it. Should I also bump the version?"}]}}"#;
    const DONE: &str = r#"{"type":"assistant","cwd":"/home/me/code/other","timestamp":"2026-09-29T09:00:00Z","message":{"id":"m","content":[{"type":"text","text":"All done."}]}}"#;

    #[test]
    fn groups_sessions_into_floors_and_queues_questions() {
        let home = Path::new("/home/me");
        let d = assemble(
            home,
            claude_inputs(vec![
                found("s1", "/home/me/code/app", &[ASK], true),
                found("s2", "/home/me/code/app", &[PROSE], true),
                found("s3", "/home/me/code/other", &[DONE], false),
            ]),
            Path::to_path_buf,
        );
        assert_eq!(d.projects.len(), 2);
        let app = d
            .projects
            .iter()
            .find(|p| p.name == "app")
            .expect("app floor");
        assert_eq!(app.path, "~/code/app");
        assert_eq!(app.session_ids, vec!["s1".to_string(), "s2".to_string()]);

        // An ended session leaves the office, but its recent floor stays.
        assert!(d.sessions.iter().all(|s| s.id != "s3"));
        let other = d
            .projects
            .iter()
            .find(|p| p.name == "other")
            .expect("other floor");
        assert!(other.session_ids.is_empty());

        assert_eq!(d.questions.len(), 2);
        let formal = &d.questions[0];
        assert_eq!(formal.id, "s1:toolu_1");
        assert_eq!(
            formal
                .options
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "B"]
        );
        assert_eq!(formal.answer_via, AnswerVia::Terminal);
        let prose = &d.questions[1];
        assert_eq!(prose.prompt, "Fixed it. Should I also bump the version?");
        assert!(prose.options.is_empty());
        let s1 = d.sessions.iter().find(|s| s.id == "s1").expect("s1");
        assert_eq!(s1.pending_question_ids, vec!["s1:toolu_1".to_string()]);
        assert_eq!(s1.control, ControlMode::RaiseWindow);
    }

    #[test]
    fn a_question_only_the_registry_knows_about_still_waits() {
        // Claude Code writes an open AskUserQuestion to the transcript only once it is answered.
        let mut asking = found("s1", "/home/me/code/app", &[DONE], true);
        if let Some(live) = asking.live.as_mut() {
            live.waiting = true;
        }
        let d = assemble(
            Path::new("/home/me"),
            claude_inputs(vec![asking]),
            Path::to_path_buf,
        );
        let s1 = d.sessions.iter().find(|s| s.id == "s1").expect("s1");
        assert_eq!(s1.state, SessionState::WaitingHuman);
        assert_eq!(s1.pending_question_ids, vec!["s1:waiting".to_string()]);
        let q = &d.questions[0];
        assert_eq!(q.prompt, "Waiting for you in its terminal");
        assert!(q.options.is_empty());
        assert_eq!(q.answer_via, AnswerVia::Terminal);
    }

    #[test]
    fn codex_sessions_join_the_same_floor_with_tool_kind_and_prose_questions() {
        let meta = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"c1","session_id":"root1","cwd":"/home/me/code/app","source":"cli"}}"#;
        let user = r#"{"timestamp":"2026-09-29T10:00:01.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix it"}]}}"#;
        let reply = r#"{"timestamp":"2026-09-29T10:00:02.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Fixed it. Should I also bump the version?"}]}}"#;
        let codex_transcript =
            super::codex::parse_transcript(Cursor::new([meta, user, reply].join("\n")));
        let codex_found = super::codex::Found {
            session_id: "c1".to_string(),
            transcript: codex_transcript,
            title: Some("Bump check".to_string()),
            live_pid: Some(7),
        };
        let idle: &str = r#"{"type":"assistant","cwd":"/home/me/code/app","timestamp":"2026-09-29T09:00:00Z","message":{"id":"m","content":[{"type":"text","text":"All good."}]}}"#;
        let home = Path::new("/home/me");
        let mut inputs = claude_inputs(vec![found("s1", "/home/me/code/app", &[idle], true)]);
        inputs.extend(from_codex(codex_found));
        let d = assemble(home, inputs, Path::to_path_buf);

        assert_eq!(d.projects.len(), 1);
        let app = &d.projects[0];
        assert_eq!(app.session_ids, vec!["s1".to_string(), "c1".to_string()]);

        let c1 = d.sessions.iter().find(|s| s.id == "c1").expect("c1");
        assert_eq!(c1.tool, ToolKind::Codex);
        assert_eq!(c1.title, "Bump check");
        assert_eq!(c1.state, SessionState::WaitingHuman);
        assert_eq!(c1.control, ControlMode::RaiseWindow);

        let prose = d
            .questions
            .iter()
            .find(|q| q.session_id == "c1")
            .expect("codex question");
        assert_eq!(prose.prompt, "Fixed it. Should I also bump the version?");
        assert!(prose.options.is_empty());
        assert_eq!(prose.answer_via, AnswerVia::Terminal);
    }

    #[test]
    fn paths_and_ids() {
        assert_eq!(
            display_path(Path::new("/home/me"), Path::new("/home/me")),
            "~"
        );
        assert_eq!(
            display_path(Path::new("/opt/x"), Path::new("/home/me")),
            "/opt/x"
        );
        assert_eq!(fnv1a(""), 0x811c_9dc5);
        assert_eq!(fnv1a("a"), 0xe40c_292c);
        assert_eq!(project_id(Path::new("/a")), project_id(Path::new("/a")));
        let here = std::env::current_dir().expect("cwd");
        assert!(project_root(&here).join(".git").exists());
    }
}
