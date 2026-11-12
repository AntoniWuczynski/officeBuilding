//! Pure helpers that turn Codex's items into the text and diffs an entry shows.

use super::ask::lines;
use super::wire::{
    FileUpdateChange, McpContent, McpResult, PatchKind, PlanNote, StepStatus, UserInput,
};
use crate::structured::FileDiff;

/// A tool title naming the files, and one diff per file.
pub(super) fn patch_view(changes: &[FileUpdateChange]) -> (String, Vec<FileDiff>) {
    let names: Vec<String> = changes
        .iter()
        .map(|c| match &c.kind {
            PatchKind::Update {
                move_path: Some(to),
            } => format!("{} → {to}", c.path),
            _ => c.path.clone(),
        })
        .collect();
    let verb = match changes {
        [c] if matches!(c.kind, PatchKind::Add) => "Add",
        [c] if matches!(c.kind, PatchKind::Delete) => "Delete",
        _ => "Edit",
    };
    let diffs = changes
        .iter()
        .map(|c| FileDiff {
            path: c.path.clone(),
            text: c.diff.clone(),
        })
        .collect();
    (format!("{verb} {}", names.join(", ")), diffs)
}

pub(super) fn plan_text(n: &PlanNote) -> String {
    let steps = n.plan.iter().map(|s| {
        let mark = match s.status {
            StepStatus::Completed => "[x]",
            StepStatus::InProgress => "[~]",
            StepStatus::Pending => "[ ]",
        };
        Some(format!("- {mark} {}", s.step))
    });
    lines(std::iter::once(n.explanation.clone()).chain(steps))
}

pub(super) fn user_text(content: Vec<UserInput>) -> String {
    content
        .into_iter()
        .filter_map(|c| match c {
            UserInput::Text { text } => Some(text),
            UserInput::Other => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn mcp_text(r: McpResult) -> String {
    r.content
        .into_iter()
        .filter_map(|c| match c {
            McpContent::Text { text } => Some(text),
            McpContent::Other => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
