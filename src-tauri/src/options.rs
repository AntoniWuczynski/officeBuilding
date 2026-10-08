//! What the app can hire: tools, their models, and each model's efforts. The
//! pickers follow each tool's own data, so new models appear without a release.
//!
//! Sources:
//! * Claude Code: the newest `~/.claude/cache/model-catalog/*.json` whose
//!   `catalog.surface` is `cc` (models, efforts and the selected defaults), then
//!   `~/.claude/settings.json` (`model`, `effortLevel`, `modelSettings.<id>`,
//!   `ultracode`). Without a catalogue a built-in list of the current family is used.
//! * Codex: `~/.codex/models_cache.json` lists models with their own
//!   `supported_reasoning_levels`; `~/.codex/config.toml` holds the defaults.
//! * Antigravity: `agy models` (one `Base name (Effort)` per line, run through the
//!   login shell, capped at 20 s, cached for 10 minutes) and
//!   `~/.gemini/antigravity-cli/settings.json` (`model`).
//! * opencode: `opencode models [provider]` exists (<https://opencode.ai/docs/cli/>)
//!   but its output format is not documented, so it is not parsed.
//! * Cursor: `--list-models` / `agent models` exist
//!   (<https://cursor.com/docs/cli/reference/parameters>) but their output format
//!   is not documented, so it is not parsed.
//!
//! Tools whose models cannot be listed get a single "Tool default" model.

use crate::model::{EffortOption, ModelOption, ToolKind, ToolOptions};
use crate::{pty, tools};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const LIST_TIMEOUT: Duration = Duration::from_secs(20);
const LIST_CACHE_TTL: Duration = Duration::from_mins(10);
const FAILURE_CACHE_TTL: Duration = Duration::from_mins(1);

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

fn default_effort_option() -> EffortOption {
    EffortOption {
        id: "default".into(),
        label: "Default".into(),
        description: String::new(),
    }
}

fn unavailable(tool: ToolKind) -> ToolOptions {
    ToolOptions {
        tool,
        available: false,
        models: Vec::new(),
        default_model: String::new(),
        ultracode: None,
    }
}

/// An installed tool whose models cannot be listed: one model, no flags passed.
fn tool_default(tool: ToolKind) -> ToolOptions {
    ToolOptions {
        tool,
        available: true,
        models: vec![ModelOption {
            id: "default".into(),
            label: "Tool default".into(),
            efforts: vec![default_effort_option()],
            default_effort: "default".into(),
        }],
        default_model: "default".into(),
        ultracode: None,
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

// ---- Claude Code ----

const CLAUDE_MODELS: &[(&str, &str, &str)] = &[
    // (id, label, alias accepted by `claude --model`)
    ("claude-opus-5-5", "Opus 5.5", "opus"),
    ("claude-fable-5-1", "Fable 5.1", "fable"),
    ("claude-sonnet-5-5", "Sonnet 5.5", "sonnet"),
    ("claude-haiku-5-5", "Haiku 5.5", "haiku"),
];
const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

#[derive(Deserialize)]
struct CatalogFile {
    #[serde(rename = "fetchedAt")]
    fetched_at: u64,
    catalog: Catalog,
}

#[derive(Deserialize)]
struct Catalog {
    surface: String,
    config: CatalogConfig,
    state: CatalogState,
}

#[derive(Deserialize)]
struct CatalogConfig {
    models: Vec<CatalogModel>,
}

#[derive(Deserialize)]
struct CatalogModel {
    id: String,
    name: String,
    #[serde(default)]
    short_name: String,
    section: String,
    thinking: Option<CatalogThinking>,
}

#[derive(Deserialize)]
struct CatalogThinking {
    #[serde(default)]
    effort_options: Vec<CatalogEffort>,
}

#[derive(Deserialize)]
struct CatalogEffort {
    id: String,
    name: String,
    badge: Option<CatalogBadge>,
}

#[derive(Deserialize)]
struct CatalogBadge {
    message: String,
}

#[derive(Deserialize, Default)]
struct CatalogState {
    model: Option<String>,
    thinking: Option<ThinkingChoice>,
    #[serde(default)]
    thinking_by_model: Vec<ModelThinking>,
}

#[derive(Deserialize)]
struct ModelThinking {
    id: String,
    thinking: ThinkingChoice,
}

#[derive(Deserialize)]
struct ThinkingChoice {
    effort: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ClaudeSettings {
    model: Option<String>,
    effort_level: Option<String>,
    #[serde(default)]
    model_settings: HashMap<String, ModelSetting>,
    #[serde(default)]
    ultracode: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelSetting {
    effort_level: Option<String>,
}

fn newest_cc_catalogue(dir: &Path) -> Option<Catalog> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(|e| read_json::<CatalogFile>(&e.ok()?.path()))
        .filter(|f| f.catalog.surface == "cc")
        .max_by_key(|f| f.fetched_at)
        .map(|f| f.catalog)
}

fn fallback_catalogue() -> Catalog {
    let efforts = CLAUDE_EFFORTS
        .iter()
        .map(|e| CatalogEffort {
            id: (*e).into(),
            name: effort_label(e),
            badge: None,
        })
        .collect::<Vec<_>>();
    let models = CLAUDE_MODELS
        .iter()
        .map(|(id, name, alias)| CatalogModel {
            id: (*id).into(),
            name: (*name).into(),
            short_name: (*alias).into(),
            section: "main".into(),
            thinking: Some(CatalogThinking {
                effort_options: efforts
                    .iter()
                    .map(|e| CatalogEffort {
                        id: e.id.clone(),
                        name: e.name.clone(),
                        badge: None,
                    })
                    .collect(),
            }),
        })
        .collect();
    Catalog {
        surface: "cc".into(),
        config: CatalogConfig { models },
        state: CatalogState::default(),
    }
}

fn claude_model_option(m: &CatalogModel) -> ModelOption {
    let efforts: Vec<EffortOption> = m
        .thinking
        .as_ref()
        .map(|t| {
            t.effort_options
                .iter()
                .map(|e| EffortOption {
                    id: e.id.clone(),
                    label: e.name.clone(),
                    description: e
                        .badge
                        .as_ref()
                        .map(|b| b.message.clone())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    let efforts = if efforts.is_empty() {
        vec![default_effort_option()]
    } else {
        efforts
    };
    ModelOption {
        id: m.id.clone(),
        label: m.name.clone(),
        default_effort: efforts[0].id.clone(),
        efforts,
    }
}

/// Claude Code's pickers from a catalogue, its selected state and the user's settings.
fn build_claude(catalog: &Catalog, settings: &ClaudeSettings) -> ToolOptions {
    let state = &catalog.state;
    let in_catalog = |id: &str| catalog.config.models.iter().any(|m| m.id == id);
    let settings_alias = settings
        .model
        .as_deref()
        .map(|m| m.split('[').next().unwrap_or(m));
    let settings_model = settings_alias.and_then(|a| {
        catalog
            .config
            .models
            .iter()
            .find(|m| {
                m.id == a || (!m.short_name.is_empty() && m.short_name.eq_ignore_ascii_case(a))
            })
            .map(|m| m.id.clone())
    });
    let mains: Vec<&CatalogModel> = catalog
        .config
        .models
        .iter()
        .filter(|m| m.section == "main")
        .collect();
    let default_model = state
        .model
        .clone()
        .filter(|m| in_catalog(m))
        .or(settings_model)
        .or_else(|| mains.first().map(|m| m.id.clone()))
        .unwrap_or_default();
    let mut models: Vec<ModelOption> = catalog
        .config
        .models
        .iter()
        .filter(|m| m.section == "main" || m.id == default_model)
        .map(claude_model_option)
        .collect();
    for m in &mut models {
        let from_state = state
            .thinking_by_model
            .iter()
            .find(|t| t.id == m.id)
            .and_then(|t| t.thinking.effort.as_ref())
            .or_else(|| {
                (m.id == default_model)
                    .then(|| state.thinking.as_ref().and_then(|t| t.effort.as_ref()))
                    .flatten()
            });
        let from_settings = settings
            .model_settings
            .get(&m.id)
            .and_then(|s| s.effort_level.as_ref())
            .or(settings.effort_level.as_ref());
        if let Some(e) = from_state
            .into_iter()
            .chain(from_settings)
            .find(|e| m.efforts.iter().any(|x| &x.id == *e))
        {
            m.default_effort = e.clone();
        }
    }
    ToolOptions {
        tool: ToolKind::ClaudeCode,
        available: true,
        models,
        default_model,
        ultracode: Some(settings.ultracode),
    }
}

fn claude_options(home: &Path) -> ToolOptions {
    let catalog = newest_cc_catalogue(&home.join(".claude/cache/model-catalog"))
        .unwrap_or_else(fallback_catalogue);
    let settings = read_json(&home.join(".claude/settings.json")).unwrap_or_default();
    build_claude(&catalog, &settings)
}

/// The Claude CLI alias for a model id (the CLI also accepts full names).
#[must_use]
pub fn claude_alias(model_id: &str) -> &str {
    CLAUDE_MODELS
        .iter()
        .find(|(id, _, _)| *id == model_id)
        .map_or(model_id, |(_, _, alias)| alias)
}

// ---- Codex ----

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
        available: true,
        models,
        default_model,
        ultracode: None,
    })
}

// ---- Antigravity ----

/// Split `Gemini 3.6 Flash (High)` into the base name and its effort text.
fn split_effort(line: &str) -> (&str, Option<&str>) {
    let line = line.trim();
    match line.strip_suffix(')').and_then(|l| l.rsplit_once(" (")) {
        Some((base, effort)) if !base.trim().is_empty() && !effort.is_empty() => {
            (base.trim(), Some(effort))
        }
        _ => (line, None),
    }
}

fn agy_effort(text: Option<&str>) -> EffortOption {
    text.map_or_else(default_effort_option, |t| EffortOption {
        id: t.to_lowercase().replace(' ', "-"),
        label: t.to_string(),
        description: String::new(),
    })
}

/// Models from `agy models` lines, grouped by base name in first-seen order.
fn parse_agy_models(text: &str) -> Vec<ModelOption> {
    let mut models: Vec<ModelOption> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (base, effort) = split_effort(line);
        let effort = agy_effort(effort);
        if let Some(m) = models.iter_mut().find(|m| m.id == base) {
            if !m.efforts.iter().any(|e| e.id == effort.id) {
                m.efforts.push(effort);
            }
        } else {
            models.push(ModelOption {
                id: base.to_string(),
                label: base.to_string(),
                default_effort: effort.id.clone(),
                efforts: vec![effort],
            });
        }
    }
    models
}

/// Run `command` through the login shell, killed after `LIST_TIMEOUT`. Stdout on success.
fn run_capped(command: &str) -> Option<String> {
    let mut child = Command::new(pty::login_shell())
        .args(["-l", "-c", command])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    if let Ok(text) = rx.recv_timeout(LIST_TIMEOUT) {
        child.wait().ok()?.success().then_some(text)
    } else {
        let _ = child.kill();
        let _ = child.wait();
        None
    }
}

/// When `agy models` last ran, and its output (`None` if it failed).
type AgyRun = (Instant, Option<String>);

static AGY_CACHE: OnceLock<Mutex<Option<AgyRun>>> = OnceLock::new();

/// `agy models` output, reused for ten minutes, or one minute after a failure
/// so a signed-out user does not wait for the command on every open.
fn agy_models_text() -> Option<String> {
    let cache = AGY_CACHE.get_or_init(|| Mutex::new(None));
    let mut slot = cache.lock().ok()?;
    if let Some((at, text)) = slot.as_ref() {
        let ttl = if text.is_some() {
            LIST_CACHE_TTL
        } else {
            FAILURE_CACHE_TTL
        };
        if at.elapsed() < ttl {
            return text.clone();
        }
    }
    let text = run_capped("agy models");
    *slot = Some((Instant::now(), text.clone()));
    text
}

#[derive(Deserialize)]
struct AgySettings {
    model: Option<String>,
}

fn build_antigravity(listing: Option<&str>, settings_model: Option<&str>) -> ToolOptions {
    let mut models = listing.map(parse_agy_models).unwrap_or_default();
    let configured = settings_model.map(split_effort);
    if models.is_empty() {
        models = configured
            .map(|(base, effort)| {
                parse_agy_models(
                    &effort.map_or_else(|| base.to_string(), |e| format!("{base} ({e})")),
                )
            })
            .unwrap_or_default();
    }
    let default_model = configured
        .map(|(base, _)| base)
        .filter(|b| models.iter().any(|m| m.id == *b))
        .or_else(|| models.first().map(|m| m.id.as_str()))
        .unwrap_or_default()
        .to_string();
    if let Some((base, Some(effort))) = configured {
        let id = agy_effort(Some(effort)).id;
        if let Some(m) = models
            .iter_mut()
            .find(|m| m.id == base && m.efforts.iter().any(|e| e.id == id))
        {
            m.default_effort = id;
        }
    }
    ToolOptions {
        tool: ToolKind::Antigravity,
        available: true,
        models,
        default_model,
        ultracode: None,
    }
}

fn antigravity_options(home: &Path) -> ToolOptions {
    let settings: Option<AgySettings> =
        read_json(&home.join(".gemini/antigravity-cli/settings.json"));
    build_antigravity(
        agy_models_text().as_deref(),
        settings.and_then(|s| s.model).as_deref(),
    )
}

fn codex_or_default(home: &Path) -> ToolOptions {
    let cache: Option<Value> = read_json(&home.join(".codex/models_cache.json"));
    let config = fs::read_to_string(home.join(".codex/config.toml")).ok();
    cache
        .as_ref()
        .and_then(|c| codex_options(c, config.as_deref()))
        .unwrap_or_else(|| tool_default(ToolKind::Codex))
}

/// Every listed tool, in picker order, with what can be hired on this machine.
#[must_use]
pub fn hire_options(home: &Path) -> Vec<ToolOptions> {
    let installed = tools::installed();
    tools::LISTED
        .iter()
        .map(|&tool| {
            if !installed.contains(&tool) {
                return unavailable(tool);
            }
            match tool {
                ToolKind::ClaudeCode => claude_options(home),
                ToolKind::Codex => codex_or_default(home),
                ToolKind::Antigravity => antigravity_options(home),
                ToolKind::Opencode | ToolKind::Cursor => tool_default(tool),
                ToolKind::Unknown => unavailable(tool),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CATALOGUE: &str = r#"{"version":2,"fetchedAt":5,"catalog":{"surface":"cc","config":{"models":[
      {"id":"claude-opus-5-5","name":"Opus 5.5","short_name":"Opus","section":"main","thinking":{"type":"effort","effort_options":[
        {"id":"low","name":"Low"},{"id":"medium","name":"Medium","badge":{"message":"Recommended","variant":"neutral"}},{"id":"xhigh","name":"Extra"}]}},
      {"id":"claude-sonnet-5-5","name":"Sonnet 5.5","short_name":"Sonnet","section":"main"},
      {"id":"claude-opus-4-8","name":"Opus 4.8","short_name":"Opus","section":"overflow","thinking":{"effort_options":[{"id":"high","name":"High"}]}},
      {"id":"claude-sonnet-4-6","name":"Sonnet 4.6","short_name":"Sonnet","section":"overflow"}]},
      "state":{"id":"cc","model":"claude-opus-4-8","thinking":{"type":"effort","effort":"high"},
       "thinking_by_model":[{"id":"claude-opus-5-5","thinking":{"type":"effort","effort":"xhigh"}}]}}}"#;

    fn catalogue() -> Catalog {
        serde_json::from_str::<CatalogFile>(CATALOGUE)
            .expect("fixture")
            .catalog
    }

    fn ids(models: &[ModelOption]) -> Vec<&str> {
        models.iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn claude_lists_main_plus_default_with_catalogue_defaults() {
        let o = build_claude(&catalogue(), &ClaudeSettings::default());
        assert_eq!(
            ids(&o.models),
            vec!["claude-opus-5-5", "claude-sonnet-5-5", "claude-opus-4-8"]
        );
        assert_eq!(o.default_model, "claude-opus-4-8");
        assert_eq!(o.ultracode, Some(false));
        let opus = &o.models[0];
        assert_eq!(opus.default_effort, "xhigh");
        assert_eq!(opus.efforts[1].description, "Recommended");
        assert_eq!(opus.efforts[2].label, "Extra");
        assert_eq!(o.models[1].efforts, vec![default_effort_option()]);
        assert_eq!(o.models[2].default_effort, "high");
    }

    #[test]
    fn claude_falls_back_to_settings_without_state() {
        let mut c = catalogue();
        c.state = CatalogState::default();
        let s: ClaudeSettings = serde_json::from_value(json!({
            "model": "sonnet[1m]", "effortLevel": "low", "ultracode": true,
            "modelSettings": {"claude-opus-5-5": {"effortLevel": "medium"}}
        }))
        .expect("settings");
        let o = build_claude(&c, &s);
        assert_eq!(o.default_model, "claude-sonnet-5-5");
        assert_eq!(o.ultracode, Some(true));
        assert_eq!(o.models[0].default_effort, "medium");
        assert_eq!(o.models[1].default_effort, "default");
    }

    #[test]
    fn claude_fallback_list_is_current_family() {
        let o = build_claude(&fallback_catalogue(), &ClaudeSettings::default());
        assert_eq!(
            ids(&o.models),
            vec![
                "claude-opus-5-5",
                "claude-fable-5-1",
                "claude-sonnet-5-5",
                "claude-haiku-5-5"
            ]
        );
        assert_eq!(o.default_model, "claude-opus-5-5");
        assert_eq!(o.models[0].default_effort, "low");
        assert_eq!(claude_alias("claude-sonnet-5-5"), "sonnet");
        assert_eq!(claude_alias("custom"), "custom");
    }

    #[test]
    fn newest_cc_catalogue_ignores_other_surfaces() {
        let dir = std::env::temp_dir().join(format!("ob-catalog-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("dir");
        let older = CATALOGUE.replace("\"fetchedAt\":5", "\"fetchedAt\":1");
        let ccd = CATALOGUE
            .replace("\"fetchedAt\":5", "\"fetchedAt\":9")
            .replace("\"surface\":\"cc\"", "\"surface\":\"ccd\"");
        fs::write(dir.join("a.json"), older.replace("Opus 5.5", "Old")).expect("write");
        fs::write(dir.join("b.json"), CATALOGUE).expect("write");
        fs::write(dir.join("c.json"), ccd).expect("write");
        fs::write(dir.join("d.json"), "not json").expect("write");
        let c = newest_cc_catalogue(&dir).expect("catalogue");
        fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(c.config.models[0].name, "Opus 5.5");
        assert!(newest_cc_catalogue(&dir).is_none());
    }

    #[test]
    fn agy_lines_group_by_base_name() {
        let m = parse_agy_models(
            "Gemini 3.6 Flash (High)\nGemini 3.6 Flash (Low)\n\nClaude Sonnet 4.6 (Thinking)\nPlain Model\nGPT-OSS 120B (Medium)\n",
        );
        assert_eq!(
            ids(&m),
            vec![
                "Gemini 3.6 Flash",
                "Claude Sonnet 4.6",
                "Plain Model",
                "GPT-OSS 120B"
            ]
        );
        assert_eq!(m[0].efforts.len(), 2);
        assert_eq!(m[0].efforts[1].id, "low");
        assert_eq!(m[0].default_effort, "high");
        assert_eq!(m[1].efforts[0].label, "Thinking");
        assert_eq!(m[2].efforts, vec![default_effort_option()]);
        assert_eq!(agy_effort(Some("Extra High")).id, "extra-high");
    }

    #[test]
    fn antigravity_defaults_from_settings_and_failure_fallback() {
        let listing = "Gemini 3.5 Flash (Low)\nGemini 3.5 Flash (High)\nGemini 3.1 Pro (Low)";
        let o = build_antigravity(Some(listing), Some("Gemini 3.5 Flash (High)"));
        assert_eq!(o.default_model, "Gemini 3.5 Flash");
        assert_eq!(o.models[0].default_effort, "high");
        let o = build_antigravity(None, Some("Gemini 3.5 Flash (Low)"));
        assert_eq!(ids(&o.models), vec!["Gemini 3.5 Flash"]);
        assert_eq!(o.default_model, "Gemini 3.5 Flash");
        assert!(build_antigravity(None, None).models.is_empty());
    }

    #[test]
    fn unlisted_tools_get_unavailable_or_tool_default() {
        let u = unavailable(ToolKind::Cursor);
        assert!(!u.available && u.models.is_empty() && u.default_model.is_empty());
        assert_eq!(u.ultracode, None);
        let d = tool_default(ToolKind::Opencode);
        assert!(d.available);
        assert_eq!(d.models[0].label, "Tool default");
        assert_eq!(d.models[0].efforts[0].id, "default");
    }

    #[test]
    #[ignore = "reads the real home folder and runs `agy models`"]
    fn print_real_hire_options() {
        let home = dirs::home_dir().expect("home");
        println!("{:#?}", hire_options(&home));
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
