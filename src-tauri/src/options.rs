//! What the app can hire: tools, their models, and each model's efforts.
//!
//! Sources (read live, see src/api/mockHireOptions.ts for the same catalogue):
//! * Claude Code: `claude --help` lists `--model` aliases and `--effort
//!   low|medium|high|xhigh|max`; the model ids below are the current Claude
//!   family. Defaults come from `~/.claude/settings.json` (`model`,
//!   `effortLevel`, `modelSettings.<id>.effortLevel`).
//! * Codex: `~/.codex/models_cache.json` lists models with their own
//!   `supported_reasoning_levels`; `~/.codex/config.toml` holds the defaults.

use crate::model::{EffortOption, ModelOption, ToolKind, ToolOptions};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn effort_label(id: &str) -> String {
    match id {
        "minimal" => "Minimal".into(),
        "low" => "Low".into(),
        "medium" => "Medium".into(),
        "high" => "High".into(),
        "xhigh" => "Extra high".into(),
        "max" => "Max".into(),
        "ultra" => "Ultra".into(),
        other => other.to_string(),
    }
}

const CLAUDE_MODELS: &[(&str, &str, &str)] = &[
    // (id, label, alias accepted by `claude --model`)
    ("claude-opus-5-5", "Opus 5.5", "opus"),
    ("claude-fable-5-1", "Fable 5.1", "fable"),
    ("claude-sonnet-5", "Sonnet 5", "sonnet"),
    ("claude-haiku-4-5-20251001", "Haiku 4.5", "haiku"),
];
const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// Claude Code's catalogue with defaults from its settings file (if readable).
pub fn claude_options(settings: Option<&Value>) -> ToolOptions {
    let global_effort = settings
        .and_then(|s| s.get("effortLevel"))
        .and_then(Value::as_str)
        .filter(|e| CLAUDE_EFFORTS.contains(e))
        .unwrap_or("medium");
    let default_alias = settings
        .and_then(|s| s.get("model"))
        .and_then(Value::as_str)
        .map(|m| m.split('[').next().unwrap_or(m).to_string());
    let default_model = CLAUDE_MODELS
        .iter()
        .find(|(id, _, alias)| {
            default_alias
                .as_deref()
                .is_some_and(|d| d == *alias || d == *id)
        })
        .map_or(CLAUDE_MODELS[0].0, |(id, _, _)| id)
        .to_string();
    let models = CLAUDE_MODELS
        .iter()
        .map(|(id, label, _)| {
            let per_model = settings
                .and_then(|s| s.get("modelSettings"))
                .and_then(|m| m.get(*id))
                .and_then(|m| m.get("effortLevel"))
                .and_then(Value::as_str)
                .filter(|e| CLAUDE_EFFORTS.contains(e));
            ModelOption {
                id: (*id).to_string(),
                label: (*label).to_string(),
                efforts: CLAUDE_EFFORTS
                    .iter()
                    .map(|e| EffortOption {
                        id: (*e).to_string(),
                        label: effort_label(e),
                        description: String::new(),
                    })
                    .collect(),
                default_effort: per_model.unwrap_or(global_effort).to_string(),
            }
        })
        .collect();
    ToolOptions {
        tool: ToolKind::ClaudeCode,
        models,
        default_model,
    }
}

/// A top-level `key = "value"` from a TOML file (before any [table]).
pub fn toml_top_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return None;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() == key {
            let v = v.trim();
            return v
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .map(str::to_string);
        }
    }
    None
}

/// Codex's catalogue from its models cache and config. None when Codex is not set up.
#[must_use]
pub fn codex_options(cache: &Value, config: Option<&str>) -> Option<ToolOptions> {
    let models: Vec<ModelOption> = cache
        .get("models")?
        .as_array()?
        .iter()
        .filter(|m| m.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|m| {
            let efforts: Vec<EffortOption> = m
                .get("supported_reasoning_levels")?
                .as_array()?
                .iter()
                .filter_map(|l| {
                    let id = l.get("effort")?.as_str()?;
                    Some(EffortOption {
                        id: id.to_string(),
                        label: effort_label(id),
                        description: l
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect();
            let first = efforts.first()?.id.clone();
            let default_effort = m
                .get("default_reasoning_level")
                .and_then(Value::as_str)
                .filter(|d| efforts.iter().any(|e| e.id == *d))
                .map_or(first, str::to_string);
            Some(ModelOption {
                id: m.get("slug")?.as_str()?.to_string(),
                label: m
                    .get("display_name")
                    .and_then(Value::as_str)
                    .or_else(|| m.get("slug")?.as_str())?
                    .to_string(),
                efforts,
                default_effort,
            })
        })
        .collect();
    let first = models.first()?.id.clone();
    let configured_model = config.and_then(|c| toml_top_string(c, "model"));
    let configured_effort = config.and_then(|c| toml_top_string(c, "model_reasoning_effort"));
    let default_model = configured_model
        .filter(|m| models.iter().any(|x| &x.id == m))
        .unwrap_or(first);
    let models = models
        .into_iter()
        .map(|mut m| {
            if m.id == default_model
                && let Some(e) = configured_effort
                    .as_ref()
                    .filter(|e| m.efforts.iter().any(|x| &x.id == *e))
            {
                m.default_effort = e.clone();
            }
            m
        })
        .collect();
    Some(ToolOptions {
        tool: ToolKind::Codex,
        models,
        default_model,
    })
}

/// Everything hireable on this machine.
#[must_use]
pub fn hire_options(home: &Path) -> Vec<ToolOptions> {
    let settings: Option<Value> = fs::read_to_string(home.join(".claude/settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let mut out = vec![claude_options(settings.as_ref())];
    let cache: Option<Value> = fs::read_to_string(home.join(".codex/models_cache.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let config = fs::read_to_string(home.join(".codex/config.toml")).ok();
    if let Some(codex) = cache
        .as_ref()
        .and_then(|c| codex_options(c, config.as_deref()))
    {
        out.push(codex);
    }
    out
}

/// The Claude CLI alias for a model id (the CLI also accepts full names).
#[must_use]
pub fn claude_alias(model_id: &str) -> &str {
    CLAUDE_MODELS
        .iter()
        .find(|(id, _, _)| *id == model_id)
        .map_or(model_id, |(_, _, alias)| alias)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_defaults_follow_settings() {
        let s = json!({"model": "opus[1m]", "effortLevel": "medium", "modelSettings": {"claude-opus-5-5": {"effortLevel": "high"}}});
        let o = claude_options(Some(&s));
        assert_eq!(o.default_model, "claude-opus-5-5");
        let opus = o
            .models
            .iter()
            .find(|m| m.id == "claude-opus-5-5")
            .expect("opus");
        assert_eq!(opus.default_effort, "high");
        let sonnet = o
            .models
            .iter()
            .find(|m| m.id == "claude-sonnet-5")
            .expect("sonnet");
        assert_eq!(sonnet.default_effort, "medium");
        assert_eq!(
            sonnet
                .efforts
                .iter()
                .map(|e| e.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Low", "Medium", "High", "Extra high", "Max"]
        );
        assert_eq!(claude_options(None).default_model, "claude-opus-5-5");
        assert_eq!(claude_alias("claude-sonnet-5"), "sonnet");
        assert_eq!(claude_alias("custom"), "custom");
    }

    #[test]
    fn codex_reads_listed_models_and_config_defaults() {
        let cache = json!({"models": [
            {"slug": "gpt-6-astra", "display_name": "GPT-6-Astra", "visibility": "list", "default_reasoning_level": "low",
             "supported_reasoning_levels": [{"effort": "low", "description": "Fast"}, {"effort": "xhigh", "description": "Deep"}]},
            {"slug": "hidden", "visibility": "hide", "supported_reasoning_levels": [{"effort": "low"}]},
            {"slug": "gpt-5.5", "display_name": "GPT-5.5", "visibility": "list", "default_reasoning_level": "medium",
             "supported_reasoning_levels": [{"effort": "low"}, {"effort": "medium"}]}
        ]});
        let o = codex_options(
            &cache,
            Some(
                "model = \"gpt-6-astra\"\nmodel_reasoning_effort = \"xhigh\"\n[tui]\nmodel = \"x\"",
            ),
        )
        .expect("codex");
        assert_eq!(
            o.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["gpt-6-astra", "gpt-5.5"]
        );
        assert_eq!(o.default_model, "gpt-6-astra");
        assert_eq!(o.models[0].default_effort, "xhigh");
        assert_eq!(o.models[0].efforts[1].label, "Extra high");
        assert_eq!(o.models[1].default_effort, "medium");
        assert!(codex_options(&json!({"models": []}), None).is_none());
    }

    #[test]
    fn toml_top_level_only() {
        assert_eq!(toml_top_string("a = \"1\"\n[t]\nb = \"2\"", "b"), None);
        assert_eq!(toml_top_string("# c\n b = \"2\"", "b"), Some("2".into()));
        assert_eq!(toml_top_string("n = 3", "n"), None);
    }
}
