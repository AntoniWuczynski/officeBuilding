use super::tests::{instructions, library, manifest, skill};
use super::*;
use crate::feed::Feed;
use std::fs;

fn quote(s: &str) -> String {
    format!("'{s}'")
}

fn reviewer(hooks: Vec<Hook>) -> RoleSetup<'static> {
    RoleSetup {
        name: "qa-floor_1",
        instructions: "Review the diff.",
        hooks,
    }
}

fn stop_hook() -> Hook {
    Hook {
        event: HookEvent::Stop,
        matcher: None,
        command: "cat > report.json".to_string(),
        targets: Targets::All,
    }
}

#[test]
fn flags_follow_the_options() {
    let launch = Launch {
        env: Vec::new(),
        opts: vec![Opt {
            flag: "--tools",
            value: "Read".to_string(),
        }],
        flags: vec!["--strict-mcp-config"],
    };
    assert_eq!(launch.args(&quote), " --tools 'Read' --strict-mcp-config");
}

#[test]
fn a_role_gets_its_own_plugin_with_the_feed_and_its_stop_hook() {
    let (lib, dir) = library("role");
    let feed = Feed::new(dir.join("feed"));
    let l = lib
        .role_launch(
            ToolKind::ClaudeCode,
            &reviewer(vec![stop_hook()]),
            Some(&feed),
        )
        .expect("launch");
    let base = dir.join("build/roles/qa-floor_1");
    let plugin = base.join("claude-code");
    let file = base.join("claude-code-instructions.md");
    assert_eq!(
        l.args(&quote),
        format!(
            " --plugin-dir '{}' --append-system-prompt-file '{}'",
            plugin.display(),
            file.display()
        )
    );
    assert!(l.env.is_empty() && l.flags.is_empty());
    assert!(plugin.join(".claude-plugin/plugin.json").is_file());
    assert_eq!(
        fs::read_to_string(&file).expect("text"),
        "Review the diff.\n"
    );
    let hooks = fs::read_to_string(plugin.join("hooks/hooks.json")).expect("hooks");
    let parsed: serde_json::Value = serde_json::from_str(&hooks).expect("json");
    for event in crate::feed::events(ToolKind::ClaudeCode) {
        assert!(parsed["hooks"][event.name()].is_array(), "{}", event.name());
    }
    let stops = parsed["hooks"]["Stop"].as_array().expect("stop hooks");
    assert_eq!(stops.len(), 2);
    assert_eq!(stops[1]["hooks"][0]["command"], "cat > report.json");
}

#[test]
fn a_role_takes_nothing_from_the_library() {
    let (lib, dir) = library("role-bare");
    skill(&lib, "tidy", "name: tidy");
    instructions(&lib, "house", "No semicolons.");
    manifest(
        &lib,
        r#"{"mcpServers":{"brain":{"command":"npx"}},
            "hooks":{"fmt":{"event":"PostToolUse","command":"./fmt.sh"}}}"#,
    );
    let l = lib
        .role_launch(ToolKind::ClaudeCode, &reviewer(vec![stop_hook()]), None)
        .expect("launch");
    let base = dir.join("build/roles/qa-floor_1");
    let plugin = base.join("claude-code");
    assert!(!plugin.join("skills/tidy").exists());
    assert!(!plugin.join(".mcp.json").exists());
    let hooks = fs::read_to_string(plugin.join("hooks/hooks.json")).expect("hooks");
    assert!(!hooks.contains("fmt.sh") && !hooks.contains("PostToolUse"));
    let text = fs::read_to_string(base.join("claude-code-instructions.md")).expect("text");
    assert!(!text.contains("semicolons"));
    let args = l.args(&quote);
    assert!(!args.contains("tidy") && !args.contains("brain"));
    // The ordinary launch still has the library's items.
    let ordinary = lib
        .launch(ToolKind::ClaudeCode, Path::new("/nowhere"), None)
        .expect("ordinary");
    assert!(ordinary.args(&quote).contains("build/claude-code"));
}

#[test]
fn a_bad_role_name_and_another_tool_are_refused() {
    let (lib, _) = library("role-refused");
    for name in ["", "../up", "a b", "qa/x"] {
        let role = RoleSetup {
            name,
            instructions: "x",
            hooks: Vec::new(),
        };
        assert_eq!(
            lib.role_launch(ToolKind::ClaudeCode, &role, None),
            Err("that role name cannot name a folder".to_string())
        );
    }
    for tool in [ToolKind::Codex, ToolKind::Antigravity, ToolKind::Opencode] {
        assert_eq!(
            lib.role_launch(tool, &reviewer(Vec::new()), None),
            Err("only Claude Code can take a fixed job so far".to_string())
        );
    }
}
