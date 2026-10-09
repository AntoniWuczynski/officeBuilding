//! Cursor's ACP extension methods (<https://cursor.com/docs/cli/acp>, read on
//! 2026-10-09). Two block the agent until the human answers:
//! * `cursor/ask_question`: questions with options; answered with the option
//!   ids picked, or skipped, or cancelled.
//! * `cursor/create_plan`: a plan in Markdown with to-dos; accepted, rejected
//!   or cancelled.
//!
//! Three are notifications the panel shows as notes: `cursor/update_todos`,
//! `cursor/task` (a sub-agent started) and `cursor/generate_image`.

use crate::structured::{Choice, Field, FieldKind, Values};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AskQuestion {
    title: Option<String>,
    questions: Vec<Question>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Question {
    id: String,
    prompt: String,
    #[serde(default)]
    options: Vec<QuestionOption>,
}

#[derive(Debug, Deserialize)]
struct QuestionOption {
    id: String,
    label: String,
}

#[derive(Debug, Deserialize)]
struct CreatePlan {
    name: Option<String>,
    overview: Option<String>,
    plan: String,
    #[serde(default)]
    todos: Vec<Todo>,
}

#[derive(Debug, Deserialize)]
struct Todo {
    content: String,
    status: TodoStatus,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Deserialize)]
struct UpdateTodos {
    todos: Vec<Todo>,
}

#[derive(Debug, Deserialize)]
struct Task {
    description: String,
}

#[derive(Debug, Deserialize)]
struct GenerateImage {
    description: String,
}

/// What an open Cursor request needs to build its reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Pending {
    /// Per question, its id and its options as (id, label).
    Question(Vec<(String, Vec<(String, String)>)>),
    Plan,
}

/// An approval for a blocking request, before it gets its id.
pub(super) struct Ask {
    pub title: String,
    pub detail: String,
    pub fields: Vec<Field>,
    pub choices: Vec<Choice>,
    pub pending: Pending,
}

fn choice(id: &str, label: &str, allow: bool) -> Choice {
    Choice {
        id: id.to_string(),
        label: label.to_string(),
        allow,
    }
}

fn todo_lines(todos: &[Todo]) -> String {
    todos
        .iter()
        .map(|t| {
            let mark = match t.status {
                TodoStatus::Completed => "- [x] ",
                TodoStatus::InProgress => "- [ ] (in progress) ",
                TodoStatus::Pending => "- [ ] ",
                TodoStatus::Cancelled => "- [-] ",
            };
            format!("{mark}{}", t.content)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A blocking request as an approval, or `None` when it is not one (or does not parse).
pub(super) fn ask(method: &str, params: &str) -> Option<Ask> {
    match method {
        "cursor/ask_question" => {
            let q: AskQuestion = serde_json::from_str(params).ok()?;
            let fields = q
                .questions
                .iter()
                .map(|question| Field {
                    id: question.id.clone(),
                    label: question.prompt.clone(),
                    kind: FieldKind::Pick {
                        options: question.options.iter().map(|o| o.label.clone()).collect(),
                        other: false,
                    },
                    required: true,
                })
                .collect();
            let pending = Pending::Question(
                q.questions
                    .into_iter()
                    .map(|question| {
                        let options = question
                            .options
                            .into_iter()
                            .map(|o| (o.id, o.label))
                            .collect();
                        (question.id, options)
                    })
                    .collect(),
            );
            Some(Ask {
                title: q
                    .title
                    .unwrap_or_else(|| "Cursor has a question".to_string()),
                detail: String::new(),
                fields,
                choices: vec![
                    choice("answer", "Send", true),
                    choice("skip", "Skip", false),
                    choice("cancel", "Cancel", false),
                ],
                pending,
            })
        }
        "cursor/create_plan" => {
            let p: CreatePlan = serde_json::from_str(params).ok()?;
            let todos = todo_lines(&p.todos);
            let detail = [p.overview.unwrap_or_default(), p.plan, todos]
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            Some(Ask {
                title: match p.name {
                    Some(name) => format!("Cursor proposes a plan: {name}"),
                    None => "Cursor proposes a plan".to_string(),
                },
                detail,
                fields: Vec::new(),
                choices: vec![
                    choice("accept", "Accept the plan", true),
                    choice("reject", "Reject", false),
                    choice("cancel", "Cancel", false),
                ],
                pending: Pending::Plan,
            })
        }
        _ => None,
    }
}

/// A notification shown as a note, or `None` when it is not one of Cursor's.
pub(super) fn note(method: &str, params: &str) -> Option<String> {
    match method {
        "cursor/update_todos" => {
            let u: UpdateTodos = serde_json::from_str(params).ok()?;
            Some(format!("To-dos:\n{}", todo_lines(&u.todos)))
        }
        "cursor/task" => {
            let t: Task = serde_json::from_str(params).ok()?;
            Some(format!("Started a sub-agent: {}", t.description))
        }
        "cursor/generate_image" => {
            let g: GenerateImage = serde_json::from_str(params).ok()?;
            Some(format!("Making an image: {}", g.description))
        }
        _ => None,
    }
}

/// `{ "outcome": { "outcome": … } }`, the shape of ACP's own permission reply.
#[derive(Serialize)]
struct Reply<T> {
    outcome: T,
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum QuestionOutcome {
    Answered { answers: Vec<Picked> },
    Skipped,
    Cancelled,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Picked {
    question_id: String,
    selected_option_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum PlanOutcome {
    Accepted,
    Rejected,
    Cancelled,
}

/// The reply's `result` for `choice_id`, with the fields' values: the panel
/// sends each question's option label, matched back to its id here.
pub(super) fn reply(pending: &Pending, choice_id: &str, values: &Values) -> Result<String, String> {
    let json = |r: Result<String, serde_json::Error>| {
        r.map_err(|e| format!("the answer could not be written: {e}"))
    };
    match (pending, choice_id) {
        (Pending::Question(questions), "answer") => {
            let mut answers = Vec::new();
            for (id, options) in questions {
                let label = values.get(id).map(|v| v.trim()).filter(|v| !v.is_empty());
                let option = label
                    .and_then(|l| options.iter().find(|(_, o)| o == l))
                    .ok_or_else(|| "answer every question first".to_string())?;
                answers.push(Picked {
                    question_id: id.clone(),
                    selected_option_ids: vec![option.0.clone()],
                });
            }
            json(serde_json::to_string(&Reply {
                outcome: QuestionOutcome::Answered { answers },
            }))
        }
        (Pending::Question(_), "skip") => json(serde_json::to_string(&Reply {
            outcome: QuestionOutcome::Skipped,
        })),
        (Pending::Question(_), "cancel") => json(serde_json::to_string(&Reply {
            outcome: QuestionOutcome::Cancelled,
        })),
        (Pending::Plan, "accept" | "reject" | "cancel") => {
            let outcome = match choice_id {
                "accept" => PlanOutcome::Accepted,
                "reject" => PlanOutcome::Rejected,
                _ => PlanOutcome::Cancelled,
            };
            json(serde_json::to_string(&Reply { outcome }))
        }
        _ => Err("that is not one of the approval's answers".to_string()),
    }
}
