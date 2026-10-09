//! Model lists a tool prints itself, as seen with opencode 1.18.33 and Cursor
//! CLI 2026.10.01 on 2026-10-08:
//! * `opencode models`: one `provider/model` per line, no default marked.
//! * `cursor-agent models`: an `Available models` heading, then `id - Label`
//!   lines; the default's label ends in `(…default)`, e.g. `Auto (current, default)`.

use super::{default_effort_option, tool_default};
use crate::model::{ModelOption, ToolKind, ToolOptions};

fn model(id: &str, label: &str) -> ModelOption {
    ModelOption {
        id: id.to_string(),
        label: label.to_string(),
        efforts: vec![default_effort_option()],
        default_effort: "default".to_string(),
    }
}

/// opencode's models after "Tool default", which stays the default: opencode
/// marks none, and its own choice may be a provider not listed here.
fn opencode(text: &str) -> ToolOptions {
    let mut options = tool_default(ToolKind::Opencode);
    options.models.extend(
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.contains(' '))
            .map(|id| model(id, id)),
    );
    options
}

/// Cursor's models, with the one it marks as default chosen.
fn cursor(text: &str) -> ToolOptions {
    let mut default = None;
    let models: Vec<ModelOption> = text
        .lines()
        .filter_map(|l| l.trim().split_once(" - "))
        .map(|(id, label)| {
            let (label, note) = match label.rsplit_once(" (") {
                Some((name, rest)) if rest.ends_with(')') => (name, rest),
                _ => (label, ""),
            };
            if default.is_none() && note.contains("default") {
                default = Some(id.to_string());
            }
            model(id.trim(), label.trim())
        })
        .collect();
    if models.is_empty() {
        return tool_default(ToolKind::Cursor);
    }
    let mut options = tool_default(ToolKind::Cursor);
    options.default_model = default
        .or_else(|| models.first().map(|m| m.id.clone()))
        .unwrap_or_default();
    options.models = models;
    options
}

/// A tool's options from its printed list; without a list, "Tool default" only.
pub(super) fn options(tool: ToolKind, listing: Option<&str>) -> ToolOptions {
    match (tool, listing) {
        (ToolKind::Opencode, Some(text)) => opencode(text),
        (ToolKind::Cursor, Some(text)) => cursor(text),
        _ => tool_default(tool),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(o: &ToolOptions) -> Vec<&str> {
        o.models.iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn opencode_keeps_its_own_default_first() {
        let o = options(
            ToolKind::Opencode,
            Some("opencode/big-pickle\nopenrouter/x-ai/grok-5\n\n"),
        );
        assert_eq!(
            ids(&o),
            vec!["default", "opencode/big-pickle", "openrouter/x-ai/grok-5"]
        );
        assert_eq!(o.default_model, "default");
    }

    #[test]
    fn cursor_lines_give_ids_labels_and_the_marked_default() {
        let text = "Available models\n\nauto - Auto (current, default)\ngpt-5.3-codex - Codex 5.3\nclaude-opus-5-thinking-high - Claude Opus 5 1M Thinking\n";
        let o = options(ToolKind::Cursor, Some(text));
        assert_eq!(
            ids(&o),
            vec!["auto", "gpt-5.3-codex", "claude-opus-5-thinking-high"]
        );
        assert_eq!(o.models[0].label, "Auto");
        assert_eq!(o.models[2].label, "Claude Opus 5 1M Thinking");
        assert_eq!(o.default_model, "auto");
        assert_eq!(o.models[1].default_effort, "default");
    }

    #[test]
    fn no_listing_or_an_empty_one_falls_back_to_the_tool_default() {
        assert_eq!(ids(&options(ToolKind::Cursor, None)), vec!["default"]);
        assert_eq!(
            ids(&options(ToolKind::Cursor, Some("Not logged in\n"))),
            vec!["default"]
        );
    }
}
