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
        server: None,
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
        refuse_peer_messages: false,
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
            server: None,
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

#[derive(serde::Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpFile {
    mcp_servers: BTreeMap<String, McpEntry>,
}

#[derive(serde::Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpEntry {
    #[serde(rename = "type")]
    kind: String,
    command: String,
    args: Vec<String>,
    always_load: bool,
}

#[test]
fn a_role_with_a_server_gets_exactly_one_stdio_server_and_a_stale_file_goes() {
    let (lib, dir) = library("role-server");
    let exe = dir.join("Office Building.app/Contents/MacOS/office-building");
    let role = RoleSetup {
        name: "manager-p-1",
        instructions: "Coordinate.",
        hooks: Vec::new(),
        server: Some(RoleServer {
            name: "office",
            command: &exe,
            args: vec!["manager-mcp".to_string(), "/data/ipc/m.sock".to_string()],
        }),
    };
    let l = lib
        .role_launch(ToolKind::ClaudeCode, &role, None)
        .expect("launch");
    let file = dir.join("build/roles/manager-p-1/mcp.json");
    assert!(
        l.args(&quote)
            .ends_with(&format!(" --mcp-config '{}'", file.display()))
    );
    let text = fs::read_to_string(&file).expect("mcp.json");
    assert!(!text.contains("env"), "{text}");
    let parsed: McpFile = serde_json::from_str(&text).expect("shape");
    assert_eq!(
        parsed,
        McpFile {
            mcp_servers: BTreeMap::from([(
                "office".to_string(),
                McpEntry {
                    kind: "stdio".to_string(),
                    command: exe.to_string_lossy().into_owned(),
                    args: vec!["manager-mcp".to_string(), "/data/ipc/m.sock".to_string()],
                    always_load: true,
                }
            )])
        }
    );
    assert!(!l.refuse_peer_messages, "the launch options add that mark");

    let bare = RoleSetup {
        server: None,
        ..role
    };
    let l = lib
        .role_launch(ToolKind::ClaudeCode, &bare, None)
        .expect("launch");
    assert!(!file.exists(), "a stale mcp.json is removed");
    assert!(!l.args(&quote).contains("--mcp-config"));
}

#[test]
fn a_command_path_that_is_not_text_is_refused() {
    use std::os::unix::ffi::OsStrExt;
    let (lib, _) = library("role-server-bad");
    let exe = Path::new(std::ffi::OsStr::from_bytes(b"/bin/\xff"));
    let role = RoleSetup {
        name: "manager-p-2",
        instructions: "Coordinate.",
        hooks: Vec::new(),
        server: Some(RoleServer {
            name: "office",
            command: exe,
            args: Vec::new(),
        }),
    };
    assert_eq!(
        lib.role_launch(ToolKind::ClaudeCode, &role, None),
        Err("the app's own path is not valid text".to_string())
    );
}
