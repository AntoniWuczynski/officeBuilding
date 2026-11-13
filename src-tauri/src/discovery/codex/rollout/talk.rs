//! A Codex thread's visible conversation, read from its rollout for the handover brief (H2
//! spec, section 7.1). It decides what of a Codex conversation leaves for another vendor:
//! A Human's prompts come from Codex's own record of them, never from the `user` role items
//! the model saw (they also carry injected context), and no reasoning item or turn item is
//! mapped to anything. Only the fields named below are declared, so `session_meta`, with its
//! account identifiers, and a reasoning item's summary and content are never held.

use super::{Obj, find};
use crate::discovery::claude_code::chat::{KEPT, Loose, lenient, one_line};
use crate::discovery::claude_code::follow::{Fed, Follow, OPEN_TAIL};
use crate::structured::{Entry, ToolStatus, keep};
use serde::Deserialize;
use std::collections::VecDeque;
use std::path::Path;

/// A thread's visible conversation, as the brief takes it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Talk {
    pub entries: Vec<Entry>,
    /// Older parts are left out: the file was longer than the read took, or the cap dropped some.
    pub earlier: bool,
}

/// The visible conversation of the Codex thread `session_id`, read once from the last
/// `OPEN_TAIL` bytes of its rollout. Empty when the rollout is not found.
#[must_use]
pub fn conversation(home: &Path, session_id: &str) -> Talk {
    let Some(path) = find(home, session_id) else {
        return Talk::default();
    };
    let mut talker = Talker::default();
    Follow::default().read(&path, Some(OPEN_TAIL), |fed| match fed {
        Fed::Fresh { skipped } => {
            talker = Talker::default();
            talker.earlier = skipped;
        }
        Fed::Line { at, text } => talker.line(at, text),
    });
    Talk {
        entries: talker.kept.into(),
        earlier: talker.earlier,
    }
}

/// One rollout line, as the conversation reads it.
#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    kind: String,
    #[serde(default, deserialize_with = "lenient")]
    payload: Option<Obj<Payload>>,
}

/// The payloads the conversation maps, tagged by the payload's own `type`. A `response_item`
/// and an `event_msg` never share a word, and the line's `type` is checked as well. Every
/// other payload, a `reasoning` item and `session_meta` among them, reads as `Other` with
/// nothing of it kept.
#[derive(Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Payload {
    // response_item
    Message {
        #[serde(default, deserialize_with = "lenient")]
        role: String,
        #[serde(default, deserialize_with = "lenient")]
        content: Vec<Loose<Part>>,
    },
    FunctionCall {
        #[serde(default, deserialize_with = "lenient")]
        name: String,
        #[serde(default, deserialize_with = "lenient")]
        arguments: String,
        #[serde(default, deserialize_with = "lenient")]
        call_id: String,
    },
    CustomToolCall {
        #[serde(default, deserialize_with = "lenient")]
        name: String,
        #[serde(default, deserialize_with = "lenient")]
        input: String,
        #[serde(default, deserialize_with = "lenient")]
        call_id: String,
    },
    #[serde(alias = "custom_tool_call_output")]
    FunctionCallOutput {
        #[serde(default, deserialize_with = "lenient")]
        call_id: String,
        #[serde(default, deserialize_with = "lenient")]
        output: Output,
    },
    // event_msg
    ItemCompleted {
        #[serde(default, deserialize_with = "lenient")]
        item: Option<Obj<TurnItem>>,
    },
    /// Legacy history's record of what the user typed (V8).
    UserMessage {
        #[serde(default, deserialize_with = "lenient")]
        message: String,
    },
    #[serde(alias = "turn_complete")]
    TaskComplete {
        #[serde(default, deserialize_with = "lenient")]
        error: Option<Obj<TurnError>>,
    },
    #[default]
    #[serde(other)]
    Other,
}

/// A turn item. Only Codex's record of what the user typed is known: a `Reasoning` item, or
/// any other, is `Other`.
#[derive(Deserialize, Default)]
#[serde(tag = "type")]
enum TurnItem {
    UserMessage {
        #[serde(default, deserialize_with = "lenient")]
        content: Vec<Loose<Part>>,
    },
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Default)]
struct TurnError {
    #[serde(default, deserialize_with = "lenient")]
    message: String,
}

/// A tool's output: a string, or a list of parts.
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum Output {
    Text(String),
    Parts(Vec<Loose<Part>>),
    #[default]
    Missing,
}

/// One part of a message, a prompt or a tool's output.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Part {
    #[serde(alias = "output_text", alias = "text")]
    InputText {
        #[serde(default, deserialize_with = "lenient")]
        text: String,
    },
    #[serde(alias = "image", alias = "local_image")]
    InputImage {},
    #[serde(other)]
    Other,
}

/// The parts' text joined by line breaks, with `[image]` for an image.
fn text(parts: Vec<Loose<Part>>) -> String {
    parts
        .into_iter()
        .filter_map(|Loose(part)| match part? {
            Part::InputText { text } => Some(text),
            Part::InputImage {} => Some("[image]".to_owned()),
            Part::Other => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The entries of one conversation, newest last.
#[derive(Default)]
struct Talker {
    kept: VecDeque<Entry>,
    earlier: bool,
}

impl Talker {
    fn put(&mut self, entry: Entry) {
        if keep(&mut self.kept, entry, KEPT) {
            self.earlier = true;
        }
    }

    /// A prompt, a reply or a notice, when there is text in it.
    fn said(&mut self, make: fn(String, String) -> Entry, at: u64, text: String) {
        if !text.trim().is_empty() {
            self.put(make(at.to_string(), text));
        }
    }

    fn call(&mut self, at: u64, name: &str, input: &str, call_id: &str) {
        let name = if name.is_empty() { "Tool call" } else { name };
        self.put(Entry::Tool {
            id: if call_id.is_empty() {
                at.to_string()
            } else {
                format!("tool-{call_id}")
            },
            title: one_line(&format!("{name} {input}")),
            status: ToolStatus::Running,
            detail: String::new(),
            diffs: Vec::new(),
        });
    }

    fn output(&mut self, call_id: &str, output: Output) {
        if call_id.is_empty() {
            return;
        }
        let id = format!("tool-{call_id}");
        let Some(Entry::Tool { title, .. }) = self.kept.iter().find(|e| e.id() == id) else {
            return;
        };
        let title = title.clone();
        let printed = match output {
            Output::Text(text) => text,
            Output::Parts(parts) => text(parts),
            Output::Missing => String::new(),
        };
        self.put(Entry::Tool {
            id,
            title,
            status: ToolStatus::Done,
            detail: crate::pty::plain_text(&printed),
            diffs: Vec::new(),
        });
    }

    /// Map the line that starts `at` bytes into the rollout.
    fn line(&mut self, at: u64, raw: &str) {
        let Ok(Obj(line)) = serde_json::from_str::<Obj<Line>>(raw) else {
            return;
        };
        let Some(Obj(payload)) = line.payload else {
            return;
        };
        // Each payload is read only under the line type Codex writes it with.
        let response = match line.kind.as_str() {
            "response_item" => true,
            "event_msg" => false,
            _ => return,
        };
        let user = |id, text| Entry::User { id, text };
        match payload {
            Payload::Message { role, content } if response && role == "assistant" => {
                self.said(|id, text| Entry::Agent { id, text }, at, text(content));
            }
            Payload::FunctionCall {
                name,
                arguments: input,
                call_id,
            }
            | Payload::CustomToolCall {
                name,
                input,
                call_id,
            } if response => self.call(at, &name, &input, &call_id),
            Payload::FunctionCallOutput { call_id, output } if response => {
                self.output(&call_id, output);
            }
            Payload::ItemCompleted {
                item: Some(Obj(TurnItem::UserMessage { content })),
            } if !response => self.said(user, at, text(content)),
            Payload::UserMessage { message } if !response => self.said(user, at, message),
            Payload::TaskComplete {
                error: Some(Obj(error)),
            } if !response => {
                self.said(|id, text| Entry::Notice { id, text }, at, error.message);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ID, LIMIT, META, day, home, rollout_name};
    use super::*;
    use crate::handover::brief::{CONVERSATION_MAX, Facts, render};
    use crate::handover::{Tool, Work};
    use std::fs;

    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay. The markers in capitals are invented, so the tests can show
    // that what they mark goes nowhere.
    const INJECTED: &str = r#"{"timestamp":"2026-11-13T12:43:15.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>INJECTED-CONTEXT</environment_context>"}]}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const DEVELOPER: &str = r#"{"timestamp":"2026-11-13T12:43:15.000Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"DEVELOPER-INSTRUCTIONS"}]}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const PROMPT: &str = r#"{"timestamp":"2026-11-13T12:43:16.000Z","type":"event_msg","payload":{"type":"item_completed","thread_id":"0199c0de-0000-4000-8000-000000000001","turn_id":"turn-1","item":{"type":"UserMessage","id":"item-1","content":[{"type":"text","text":"Fix the importer","text_elements":[]},{"type":"local_image","path":"/work/app/shot.png"}]}}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const LEGACY_PROMPT: &str = r#"{"timestamp":"2026-11-13T12:43:16.000Z","type":"event_msg","payload":{"type":"user_message","message":"And keep the tests green","images":[]}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const REASONING: &str = r#"{"timestamp":"2026-11-13T12:43:17.000Z","type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"HIDDEN-SUMMARY"}],"content":[{"type":"text","text":"HIDDEN-CONTENT"}],"encrypted_content":"HIDDEN-BLOB"}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const REASONING_ITEM: &str = r#"{"timestamp":"2026-11-13T12:43:17.000Z","type":"event_msg","payload":{"type":"item_completed","thread_id":"0199c0de-0000-4000-8000-000000000001","turn_id":"turn-1","item":{"type":"Reasoning","id":"item-2","summary_text":["HIDDEN-ITEM"],"raw_content":["HIDDEN-RAW"]}}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const REPLY: &str = r#"{"timestamp":"2026-11-13T12:43:18.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Looking at the importer now."}]}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const PATCH: &str = r#"{"timestamp":"2026-11-13T12:43:19.000Z","type":"response_item","payload":{"type":"custom_tool_call","status":"completed","call_id":"call-patch","name":"apply_patch","input":"*** Begin Patch\n*** Update File: src/importer.rs\n*** End Patch"}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const PATCH_OUT: &str = r#"{"timestamp":"2026-11-13T12:43:19.000Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call-patch","output":[{"type":"input_text","text":"Success. Updated src/importer.rs"},{"type":"input_image","image_url":"data:image/png;base64,AAAA"}]}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const SHELL: &str = r#"{"timestamp":"2026-11-13T12:43:20.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"cargo test\"}","call_id":"call-shell"}}"#;
    // Composed from the key names of rollouts on one machine and from the Codex 0.161.0
    // source, not from a replay.
    const SHELL_OUT: &str = r#"{"timestamp":"2026-11-13T12:43:21.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call-shell","output":"test result: \u001b[32mok\u001b[0m"}}"#;

    /// The lines through a talker, each at its byte offset.
    fn talk(lines: &[&str]) -> Talker {
        let mut talker = Talker::default();
        let mut at = 0;
        for line in lines {
            talker.line(at, line);
            at += u64::try_from(line.len() + 1).expect("offset");
        }
        talker
    }

    /// An entry as (kind, title or text, detail), its id left out.
    fn shown(entry: &Entry) -> (&'static str, String, String) {
        match entry {
            Entry::User { text, .. } => ("user", text.clone(), String::new()),
            Entry::Agent { text, .. } => ("agent", text.clone(), String::new()),
            Entry::Thought { text, .. } => ("thought", text.clone(), String::new()),
            Entry::Notice { text, .. } => ("notice", text.clone(), String::new()),
            Entry::Tool {
                title,
                status,
                detail,
                ..
            } => (
                if *status == ToolStatus::Done {
                    "done"
                } else {
                    "tool"
                },
                title.clone(),
                detail.clone(),
            ),
        }
    }

    fn reply(n: usize) -> String {
        REPLY.replace("Looking at the importer now.", &format!("reply-{n:04}"))
    }

    /// A temp home whose rollout holds `lines`.
    fn rollout(name: &str, lines: &[&str]) -> std::path::PathBuf {
        let home = home(&format!("talk-{name}"));
        let file = day(&home, "2026/11/13").join(rollout_name("2026-11-13T12-43-15"));
        fs::write(&file, lines.join("\n") + "\n").expect("rollout");
        home
    }

    const EVERYTHING: [&str; 13] = [
        META,
        INJECTED,
        DEVELOPER,
        PROMPT,
        LEGACY_PROMPT,
        REASONING,
        REASONING_ITEM,
        REPLY,
        PATCH,
        PATCH_OUT,
        SHELL,
        SHELL_OUT,
        LIMIT,
    ];

    #[test]
    fn the_conversation_table() {
        let home = rollout("table", &EVERYTHING);
        let got = conversation(&home, ID);
        assert!(!got.earlier);
        let rows: Vec<_> = got.entries.iter().map(shown).collect();
        let row = |kind, text: &str, detail: &str| (kind, text.to_owned(), detail.to_owned());
        assert_eq!(
            rows,
            vec![
                row("user", "Fix the importer\n[image]", ""),
                row("user", "And keep the tests green", ""),
                row("agent", "Looking at the importer now.", ""),
                row(
                    "done",
                    "apply_patch *** Begin Patch *** Update File: src/importer.rs *** End Patch",
                    "Success. Updated src/importer.rs\n[image]"
                ),
                row(
                    "done",
                    "exec_command {\"cmd\":\"cargo test\"}",
                    "test result: ok"
                ),
                row(
                    "notice",
                    "You’ve hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), \
                     visit https://chatgpt.com/settings/usage to purchase more credits or try \
                     again at 2:41 PM.",
                    ""
                ),
            ]
        );
        assert_eq!(got.entries[3].id(), "tool-call-patch");
        assert!(
            !got.entries
                .iter()
                .any(|e| matches!(e, Entry::Thought { .. }))
        );
        fs::remove_dir_all(home).expect("cleanup");
    }

    #[test]
    fn reasoning_injected_context_and_account_identifiers_appear_nowhere_in_the_brief() {
        let home = rollout("brief", &EVERYTHING);
        let talk = conversation(&home, ID);
        let work = Work::Clear;
        let brief = render(
            &Facts {
                id: "h-13",
                from: Tool::Codex,
                to: Tool::ClaudeCode,
                by_human: false,
                at: 1_794_573_800,
                window: None,
                reset_at: None,
                title: "Fix the importer",
                folder: Path::new("/work/app"),
                floor: "app",
                branch: Some("main"),
                worktree: false,
                work: &work,
                git: "$ git status --porcelain=v1 --branch\n## main",
                earlier: talk.earlier,
            },
            &talk.entries,
        );
        assert!(
            brief.contains("\nCodex:\nLooking at the importer now."),
            "{brief}"
        );
        assert!(
            brief.contains("\nA Human:\nFix the importer\n[image]"),
            "{brief}"
        );
        for hidden in [
            "HIDDEN-SUMMARY",
            "HIDDEN-CONTENT",
            "HIDDEN-BLOB",
            "HIDDEN-ITEM",
            "HIDDEN-RAW",
            "INJECTED-CONTEXT",
            "DEVELOPER-INSTRUCTIONS",
            "user-invented-for-this-test",
            "account-invented-for-this-test",
            "base64",
        ] {
            assert!(!brief.contains(hidden), "{hidden} in {brief}");
        }
        fs::remove_dir_all(home).expect("cleanup");
    }

    #[test]
    fn the_cap_counts_from_the_end() {
        let lines: Vec<String> = (0..KEPT + 5).map(reply).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let talker = talk(&refs);
        assert!(talker.earlier);
        assert_eq!(talker.kept.len(), KEPT);
        assert_eq!(
            talker.kept.front().map(shown),
            Some(("agent", "reply-0005".to_owned(), String::new()))
        );

        let long: Vec<String> = (0..300)
            .map(|n| reply(n).replace("reply-", &format!("{}-reply-", "x".repeat(1000))))
            .collect();
        let refs: Vec<&str> = long.iter().map(String::as_str).collect();
        let home = rollout("cap", &refs);
        let talk = conversation(&home, ID);
        assert!(!talk.earlier);
        let work = Work::Clear;
        let brief = render(
            &Facts {
                id: "h-1",
                from: Tool::Codex,
                to: Tool::ClaudeCode,
                by_human: false,
                at: 0,
                window: None,
                reset_at: None,
                title: "",
                folder: Path::new("/work/app"),
                floor: "app",
                branch: None,
                worktree: false,
                work: &work,
                git: "",
                earlier: talk.earlier,
            },
            &talk.entries,
        );
        const { assert!(300 * 1000 > CONVERSATION_MAX) };
        assert!(brief.contains("-reply-0299"));
        assert!(!brief.contains("-reply-0000"));
        assert!(brief.contains("(Earlier parts of the conversation are left out.)"));
        fs::remove_dir_all(home).expect("cleanup");
    }

    #[test]
    fn outputs_calls_and_files_that_give_little_or_nothing() {
        let orphan = SHELL_OUT.replace("call-shell", "call-gone");
        assert!(talk(&[&orphan]).kept.is_empty());

        let no_id = SHELL.replace(r#","call_id":"call-shell""#, "");
        let talker = talk(&[REPLY, &no_id]);
        let at = REPLY.len() + 1;
        assert_eq!(
            talker.kept.back().map(Entry::id),
            Some(at.to_string().as_str())
        );

        let talker = talk(&[SHELL, SHELL_OUT]);
        let Some(Entry::Tool { detail, .. }) = talker.kept.back() else {
            panic!("no tool entry");
        };
        assert_eq!(detail, "test result: ok");
        assert!(!detail.contains('\x1b'));

        let nameless = PATCH.replace(r#""name":"apply_patch","#, "");
        assert_eq!(
            talk(&[&nameless]).kept.back().map(shown).map(|s| s.1),
            Some("Tool call *** Begin Patch *** Update File: src/importer.rs *** End Patch".into())
        );

        for nothing in [
            "not json",
            "[1]",
            r#"{"type":"response_item","payload":"message"}"#,
            r#"{"type":"response_item","payload":["message","assistant",[]]}"#,
            r#"{"type":"event_msg","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"x"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"user_message","message":"x"}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_complete","error":{"message":""}}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"Done."}}"#,
            r#"{"type":"event_msg","payload":{"type":"agent_reasoning","text":"HIDDEN"}}"#,
        ] {
            assert!(talk(&[nothing]).kept.is_empty(), "{nothing}");
        }
        let turn = LIMIT.replace("task_complete", "turn_complete");
        assert_eq!(talk(&[&turn]).kept.len(), 1);

        let empty = home("talk-none");
        assert_eq!(conversation(&empty, ID), Talk::default());
        assert_eq!(conversation(&empty, "not-a-uuid"), Talk::default());
        fs::remove_dir_all(empty).expect("cleanup");
    }
}
