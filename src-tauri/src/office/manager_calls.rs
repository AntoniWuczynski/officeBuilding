//! `escalate`: a call for A Human, added to the floor's human decisions file when it has one and
//! kept in the building's own records when it has not (OB-002). Also how A Human settles a call
//! the building keeps.

use super::manager_send::{log_event, may_act, refuse};
use super::manager_tools::json;
use super::{Inner, Office, answer_text, now_iso};
use crate::delivery::text;
use crate::manager::tools::Escalate;
use crate::manager::wire::LogEvent;
use crate::model::{AnswerVia, Decision, Party, Question, QuestionAnswer, QuestionOption};
use crate::project_files::{self, append};
use serde::Serialize;
use std::fs;

const TOOL: &str = "escalate";
const TITLE_MAX: usize = 120;
const QUESTION_MAX: usize = 600;
const OPTION_MAX: usize = 140;
const OPTIONS_MAX: usize = 6;
/// The most calls of a floor's manager that may wait for A Human at once.
const OPEN_CALLS_MAX: usize = 10;
/// Where a call the building keeps is said to be.
const OWN_RECORDS: &str = "the building's own records";

#[derive(Serialize)]
struct Escalated {
    escalated: String,
    r#where: String,
}

/// `raw` on one line of at most `max` characters, without `—`, `#`, `*` or backticks, so it
/// can be neither a heading, a status nor a bold option of its own.
fn plain_line(raw: &str, max: usize) -> Result<String, String> {
    let kept: String = text::line(raw, max)?
        .chars()
        .filter(|c| !matches!(c, '—' | '#' | '*' | '`'))
        .collect();
    let kept = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    if kept.is_empty() {
        return Err("there is nothing left of it once cleaned".to_string());
    }
    Ok(kept)
}

/// The question on one line. It cannot start a heading or a table row, and holds no `*`, so
/// it cannot add an option of its own.
fn question_line(raw: &str) -> Result<String, String> {
    let line: String = text::line(raw, QUESTION_MAX)?
        .chars()
        .filter(|c| *c != '*')
        .collect();
    let line = line.trim_start_matches(|c: char| c == '#' || c == '|' || c.is_whitespace());
    if line.is_empty() {
        return Err("there is nothing to ask".to_string());
    }
    Ok(line.to_string())
}

/// The call's title, question and options, cleaned.
fn checked(args: &Escalate) -> Result<(String, String, Vec<String>), String> {
    if args.options.len() > OPTIONS_MAX {
        return Err("at most six options".to_string());
    }
    let options = args
        .options
        .iter()
        .map(|option| plain_line(option, OPTION_MAX))
        .collect::<Result<Vec<_>, _>>()?;
    let title = plain_line(&args.title, TITLE_MAX)?;
    if append::reads_settled(&title) {
        return Err("a title cannot start with a word that marks a call settled".to_string());
    }
    Ok((title, question_line(&args.question)?, options))
}

impl Office {
    /// Ask A Human to decide something on the manager's floor.
    pub(super) fn escalate(&self, floor: &str, args: &Escalate) -> Result<String, String> {
        let escalated = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            may_act(
                inner,
                floor,
                TOOL,
                "the manager is paused, so nothing is escalated",
            )?;
            let added = checked(args).and_then(|(title, question, options)| {
                if Self::open_manager_calls(inner, floor) >= OPEN_CALLS_MAX {
                    return Err(format!(
                        "{OPEN_CALLS_MAX} of your calls still wait for the human, so ask nothing more until they answer"
                    ));
                }
                let (id, place) = Self::add_call(inner, floor, &title, &question, &options)?;
                Ok((format!("MGR-{id}: {title}"), place, title))
            });
            let (escalated, place, title) =
                added.map_err(|reason| refuse(inner, floor, TOOL, reason))?;
            log_event(inner, floor, LogEvent::Escalation { title });
            // The call is made either way; the log is caught up at the next save.
            let _ = inner.store.save();
            Escalated {
                escalated,
                r#where: place,
            }
        };
        if escalated.r#where != OWN_RECORDS {
            self.refresh();
        }
        json(&escalated)
    }

    /// How many calls of the manager wait for A Human on `floor`, in its file and in the store.
    fn open_manager_calls(inner: &Inner, floor: &str) -> usize {
        let mine = |q: &&Question| q.prompt.starts_with("MGR-");
        let in_file = inner.found.roots.get(floor).map_or(0, |root| {
            project_files::read(root, floor)
                .open
                .iter()
                .filter(mine)
                .count()
        });
        let kept = inner
            .store
            .records
            .calls
            .iter()
            .filter(|q| q.project_id == floor);
        in_file + kept.filter(mine).count()
    }

    /// Add the call to the floor's decisions file, or to the store when the repo has none.
    /// Returns its number and where it went.
    fn add_call(
        inner: &mut Inner,
        floor: &str,
        title: &str,
        question: &str,
        options: &[String],
    ) -> Result<(String, String), String> {
        let root = inner
            .found
            .roots
            .get(floor)
            .cloned()
            .ok_or_else(|| "your floor is no longer in the building".to_string())?;
        if let Some(file) = append::decisions_file(&root) {
            let text = fs::read_to_string(root.join(&file))
                .map_err(|e| format!("could not read {file}: {e}"))?;
            let id = append::next_call_id(&text);
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            append::append(
                &root,
                &file,
                &append::call_section(id, title, question, options, &today),
            )?;
            return Ok((id.to_string(), file));
        }
        let id = inner.store.next_id("c");
        let seq = id.trim_start_matches("c-").to_string();
        inner.store.records.calls.push(Question {
            id,
            project_id: floor.to_string(),
            session_id: String::new(),
            prompt: format!("MGR-{seq}: {title}"),
            options: options
                .iter()
                .enumerate()
                .map(|(i, label)| QuestionOption {
                    id: format!("opt-{i}"),
                    label: label.clone(),
                })
                .collect(),
            allow_other: true,
            answer_via: AnswerVia::Store,
            priority: 0,
            asked_at: now_iso(),
            context: question.to_string(),
            source_file: None,
        });
        inner.store.save()?;
        Ok((seq, OWN_RECORDS.to_string()))
    }

    /// Settle a call the building keeps: it becomes A Human's decision. None when
    /// `question_id` is not one of those calls.
    pub(super) fn answer_store_call(
        &self,
        question_id: &str,
        answer: &QuestionAnswer,
    ) -> Option<Result<(), String>> {
        let mut inner = self.lock();
        let at = inner
            .store
            .records
            .calls
            .iter()
            .position(|q| q.id == question_id)?;
        let detail = match answer_text(&inner.store.records.calls[at], answer) {
            Ok(detail) => detail,
            Err(reason) => return Some(Err(reason)),
        };
        let call = inner.store.records.calls.remove(at);
        let id = inner.store.next_id("d");
        inner.store.records.decisions.push(Decision {
            id,
            project_id: call.project_id,
            title: call.prompt,
            detail,
            decided_at: now_iso(),
            by: Party::Human,
        });
        Some(inner.store.save())
    }
}

#[cfg(test)]
mod tests;
