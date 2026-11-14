//! Which tools have a chat view, and the view a hire opens in.

use super::super::{Inner, Office};
use crate::model::{ChatSupport, DeskView, HireRequest, Session, ToolKind, ToolOptions};
use crate::options;
use crate::structured;

/// Whether `tool`'s desks can open in the chat view, or why not. `available` is
/// whether the tool is installed.
pub(in crate::office) fn support(tool: ToolKind, available: bool) -> ChatSupport {
    let unavailable = |reason: &str| ChatSupport::Unavailable {
        reason: reason.to_string(),
    };
    if !available {
        return unavailable("it is not installed on this Mac");
    }
    if structured::supports(tool) {
        return ChatSupport::Available { terminal: false };
    }
    match tool {
        ToolKind::Codex | ToolKind::Opencode | ToolKind::Cursor => {
            unavailable("it has no structured driver")
        }
        ToolKind::ClaudeCode => ChatSupport::Available { terminal: true },
        ToolKind::Antigravity => {
            unavailable("chat for Antigravity is not built yet, so it runs in the terminal")
        }
        ToolKind::Unknown => unavailable("this tool has no chat view"),
    }
}

/// Set every tool's chat support, and the view the hire dialog starts on:
/// `default` where chat is available, else the terminal.
pub(in crate::office) fn fill_views(options: &mut [ToolOptions], default: DeskView) {
    for o in options {
        o.chat = support(o.tool, o.available);
        o.view = match o.chat {
            ChatSupport::Available { .. } => default,
            ChatSupport::Unavailable { .. } => DeskView::Terminal,
        };
    }
}

impl Office {
    /// The Settings choice for new desks.
    fn default_view(&self) -> DeskView {
        self.settings
            .as_ref()
            .map_or(DeskView::Terminal, |s| s.get().agents.view)
    }

    pub fn hire_options(&self) -> Vec<ToolOptions> {
        let mut all = options::hire_options(&self.home);
        fill_views(&mut all, self.default_view());
        all
    }

    /// Start an agent in an in-app terminal, in the floor's folder, or a chat
    /// desk the building drives itself. It appears as a desk as soon as the
    /// tool writes its session files.
    pub fn spawn_session(&self, project_id: &str, req: &HireRequest) -> Result<Session, String> {
        if req.view == DeskView::Chat
            && let ChatSupport::Unavailable { reason } = support(req.tool, true)
        {
            return Err(reason);
        }
        let (session, terminal) = self.spawn_in_terminal(project_id, req)?;
        if let (DeskView::Chat, Some(terminal)) = (req.view, terminal) {
            self.lock().chat.terminals.insert(terminal);
        }
        Ok(session)
    }

    /// Record `terminal` as a chat terminal when `tool` opens in chat by default.
    pub(in crate::office) fn note_view(&self, tool: ToolKind, terminal: &str) {
        if tool == ToolKind::ClaudeCode
            && self.default_view() == DeskView::Chat
            && matches!(support(tool, true), ChatSupport::Available { .. })
        {
            self.lock().chat.terminals.insert(terminal.to_string());
        }
    }

    /// The sessions in chat terminals, by session id.
    pub(in crate::office) fn chat_desks(inner: &Inner) -> Vec<String> {
        inner
            .owned
            .iter()
            .filter(|(terminal, _)| inner.chat.terminals.contains(*terminal))
            .map(|(_, session)| session.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::office::tests::floor_office;

    const ALL: [ToolKind; 6] = [
        ToolKind::ClaudeCode,
        ToolKind::Codex,
        ToolKind::Opencode,
        ToolKind::Antigravity,
        ToolKind::Cursor,
        ToolKind::Unknown,
    ];

    fn reason(support: ChatSupport) -> Option<String> {
        match support {
            ChatSupport::Unavailable { reason } => Some(reason),
            ChatSupport::Available { .. } => None,
        }
    }

    #[test]
    fn support_follows_the_table_for_every_tool() {
        for tool in ALL {
            assert_eq!(
                reason(support(tool, false)).as_deref(),
                Some("it is not installed on this Mac"),
                "{tool:?} not installed"
            );
        }
        for tool in [ToolKind::Codex, ToolKind::Opencode, ToolKind::Cursor] {
            assert_eq!(
                support(tool, true),
                ChatSupport::Available { terminal: false }
            );
        }
        assert_eq!(
            support(ToolKind::ClaudeCode, true),
            ChatSupport::Available { terminal: true }
        );
        let words = |tool| reason(support(tool, true));
        assert_eq!(
            words(ToolKind::Antigravity).as_deref(),
            Some("chat for Antigravity is not built yet, so it runs in the terminal")
        );
        assert_eq!(
            words(ToolKind::Unknown).as_deref(),
            Some("this tool has no chat view")
        );
    }

    #[test]
    fn fill_views_gives_the_default_only_where_chat_is_available() {
        let mut all: Vec<ToolOptions> = ALL
            .iter()
            .map(|t| ToolOptions::new(*t, true, Vec::new(), String::new(), None))
            .collect();
        all[2].available = false;
        fill_views(&mut all, DeskView::Chat);
        let views: Vec<DeskView> = all.iter().map(|o| o.view).collect();
        assert_eq!(
            views,
            [
                DeskView::Chat,
                DeskView::Chat,
                DeskView::Terminal,
                DeskView::Terminal,
                DeskView::Chat,
                DeskView::Terminal,
            ]
        );
        assert_eq!(
            all[2].chat,
            ChatSupport::Unavailable {
                reason: "it is not installed on this Mac".to_string()
            }
        );
        fill_views(&mut all, DeskView::Terminal);
        assert!(all.iter().all(|o| o.view == DeskView::Terminal));
    }

    #[test]
    fn chat_desks_lists_the_sessions_in_chat_terminals_and_follows_a_new_session() {
        let (office, dir) = crate::office::tests::office("chat-desks");
        {
            let mut inner = office.lock();
            inner.owned.insert("t-1".into(), "s-1".into());
            inner.owned.insert("t-2".into(), "s-2".into());
            inner.chat.terminals.insert("t-1".into());
            inner.chat.terminals.insert("t-9".into());
            assert_eq!(Office::chat_desks(&inner), ["s-1"]);
            inner.owned.insert("t-1".into(), "s-3".into());
            assert_eq!(Office::chat_desks(&inner), ["s-3"]);
        }
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    // `hire_options` itself lists the installed tools' models by running them, which
    // tests must not do, so this checks the default it hands to `fill_views`.
    #[test]
    fn with_chat_in_settings_claude_code_opens_in_chat_and_its_terminal_is_a_chat_desk() {
        use crate::settings::{AgentSettings, SettingsPatch, SettingsStore};
        let (office, dir) = crate::office::tests::office("chat-default");
        let settings = std::sync::Arc::new(SettingsStore::open(dir.join("settings.json")));
        let patch = SettingsPatch {
            agents: Some(AgentSettings {
                view: DeskView::Chat,
            }),
            ..SettingsPatch::default()
        };
        settings.update(patch).expect("save");
        let office = office.with_settings(settings);
        let mut claude =
            ToolOptions::new(ToolKind::ClaudeCode, true, Vec::new(), String::new(), None);
        fill_views(std::slice::from_mut(&mut claude), office.default_view());
        assert_eq!(claude.view, DeskView::Chat);
        office.note_view(ToolKind::ClaudeCode, "t1");
        office.lock().owned.insert("t1".into(), "s1".into());
        assert_eq!(Office::chat_desks(&office.lock()), ["s1"]);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_chat_hire_of_antigravity_fails_with_its_reason_and_starts_nothing() {
        let (office, dir, floor) = floor_office("chat-agy");
        let req = HireRequest {
            tool: ToolKind::Antigravity,
            title: "write docs".to_string(),
            model: "default".to_string(),
            effort: "default".to_string(),
            ultracode: false,
            view: DeskView::Chat,
        };
        assert_eq!(
            office.spawn_session(&floor.id, &req),
            Err("chat for Antigravity is not built yet, so it runs in the terminal".to_string())
        );
        assert!(office.terminals.running_children().is_empty());
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_hire_request_defaults_to_the_terminal_view() {
        let json = |view: &str| {
            format!(r#"{{"tool":"codex","title":"t","model":"m","effort":"e"{view}}}"#)
        };
        let plain: HireRequest = serde_json::from_str(&json("")).expect("plain");
        assert_eq!(plain.view, DeskView::Terminal);
        let chat: HireRequest = serde_json::from_str(&json(r#","view":"chat""#)).expect("chat");
        assert_eq!(chat.view, DeskView::Chat);
    }

    #[test]
    fn tool_options_serialise_chat_and_view() {
        let mut options = ToolOptions::new(ToolKind::Codex, true, Vec::new(), String::new(), None);
        fill_views(std::slice::from_mut(&mut options), DeskView::Terminal);
        let value = serde_json::to_value(&options).expect("json");
        assert_eq!(
            value["chat"],
            serde_json::json!({"kind":"available","terminal":false})
        );
        assert_eq!(value["view"], "terminal");
    }
}
