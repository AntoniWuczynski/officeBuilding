use super::*;
use std::fs;

fn library(tag: &str) -> (Library, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ob-setup-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let lib = Library::new(dir.join("Setup"), dir.join("build"));
    lib.ensure().expect("ensure");
    (lib, dir)
}

fn skill(lib: &Library, name: &str, front: &str) {
    let dir = lib.dir().join("skills").join(name);
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(dir.join("SKILL.md"), format!("---\n{front}\n---\nBody\n")).expect("write");
}

fn manifest(lib: &Library, json: &str) {
    fs::write(lib.dir().join("setup.json"), json).expect("write");
}

fn quote(s: &str) -> String {
    format!("'{s}'")
}

/// The command-line text a launch adds, with no `~/.codex` to overlay.
fn launch(lib: &Library, tool: ToolKind) -> Result<String, String> {
    let l = lib.launch(tool, Path::new("/nowhere"), None)?;
    Ok(format!("{}{}", l.prefix(&quote), l.args(&quote)))
}

#[test]
fn a_new_library_has_an_empty_manifest_and_a_guide_for_agents() {
    let (lib, _) = library("new");
    let view = lib.view(Path::new("/nowhere"), &[]);
    assert!(view.items.is_empty() && view.problems.is_empty());
    assert!(lib.dir().join("AGENTS.md").is_file());
    assert_eq!(
        fs::read_to_string(lib.dir().join("CLAUDE.md")).expect("read"),
        "@AGENTS.md\n"
    );
}

#[test]
fn descriptions_are_read_from_plain_and_block_front_matter() {
    assert_eq!(
        front_matter_description("---\nname: a\ndescription: Plain words\n---\n"),
        "Plain words"
    );
    assert_eq!(
        front_matter_description(
            "---\nname: a\ndescription: |\n  First line\n  second line\nversion: 1\n---\n"
        ),
        "First line second line"
    );
    assert_eq!(
        front_matter_description("---\ndescription: \"Quoted\"\n---\n"),
        "Quoted"
    );
    assert_eq!(front_matter_description("No front matter"), "");
}

#[test]
fn a_skill_with_no_entry_applies_everywhere_and_targets_are_saved() {
    let (lib, _) = library("targets");
    skill(&lib, "tidy", "name: tidy\ndescription: Tidies");
    let view = lib.view(Path::new("/nowhere"), &[]);
    assert_eq!(view.items[0].targets, Targets::All);
    let only = Targets::Only {
        tools: vec![ToolKind::Antigravity],
    };
    lib.set_targets(ItemKind::Skill, "tidy", only.clone())
        .expect("set");
    assert_eq!(lib.view(Path::new("/nowhere"), &[]).items[0].targets, only);
    assert!(
        lib.set_targets(ItemKind::Skill, "missing", Targets::All)
            .is_err()
    );
}

#[test]
fn a_broken_manifest_is_reported_and_never_overwritten() {
    let (lib, _) = library("broken");
    manifest(&lib, "{ not json");
    assert_eq!(lib.view(Path::new("/nowhere"), &[]).problems.len(), 1);
    skill(&lib, "tidy", "name: tidy");
    assert!(
        lib.set_targets(ItemKind::Skill, "tidy", Targets::All)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(lib.dir().join("setup.json")).expect("read"),
        "{ not json"
    );
}

#[test]
fn an_mcp_server_name_that_is_not_a_bare_key_is_skipped() {
    let (lib, _) = library("names");
    manifest(
        &lib,
        r#"{"mcpServers":{"ok_1":{"command":"a"},"bad name":{"command":"b"}}}"#,
    );
    let view = lib.view(Path::new("/nowhere"), &[]);
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.items[0].name, "ok_1");
    assert_eq!(view.problems.len(), 1);
}

#[test]
fn claude_code_gets_a_plugin_folder_with_its_skills_and_mcp_servers() {
    let (lib, dir) = library("claude");
    skill(&lib, "tidy", "name: tidy");
    skill(&lib, "agy-only", "name: agy-only");
    manifest(
        &lib,
        r#"{"skills":{"agy-only":{"targets":{"kind":"only","tools":["antigravity"]}}},
            "mcpServers":{"brain":{"command":"npx","args":["-y","b"],"env":{"K":"v"}}}}"#,
    );
    let args = launch(&lib, ToolKind::ClaudeCode).expect("args");
    let plugin = dir.join("build/claude-code");
    assert_eq!(args, format!(" --plugin-dir '{}'", plugin.display()));
    assert!(plugin.join(".claude-plugin/plugin.json").is_file());
    assert_eq!(
        fs::read_link(plugin.join("skills/tidy")).expect("link"),
        lib.dir().join("skills/tidy")
    );
    assert!(!plugin.join("skills/agy-only").exists());
    let mcp = fs::read_to_string(plugin.join(".mcp.json")).expect("mcp");
    assert!(
        mcp.contains("\"mcpServers\"") && mcp.contains("\"npx\"") && mcp.contains("\"K\": \"v\"")
    );
}

#[test]
fn a_skill_aimed_elsewhere_later_loses_its_link() {
    let (lib, dir) = library("relink");
    skill(&lib, "tidy", "name: tidy");
    launch(&lib, ToolKind::Antigravity).expect("args");
    let link = dir.join("build/antigravity/.agents/skills/tidy");
    assert!(link.exists());
    let only = Targets::Only {
        tools: vec![ToolKind::ClaudeCode],
    };
    lib.set_targets(ItemKind::Skill, "tidy", only).expect("set");
    assert_eq!(launch(&lib, ToolKind::Antigravity).expect("args"), "");
    // Nothing is aimed at it, so the folder is not even passed; a running agent's
    // stale link is harmless, but a new launch would not use it.
    skill(&lib, "other", "name: other");
    launch(&lib, ToolKind::Antigravity).expect("args");
    assert!(fs::symlink_metadata(&link).is_err());
}

#[test]
fn codex_gets_each_mcp_server_as_config_overrides() {
    let (lib, _) = library("codex");
    manifest(
        &lib,
        r#"{"mcpServers":{"brain":{"command":"npx","args":["-y","b"],"env":{"K":"v"}}}}"#,
    );
    let args = launch(&lib, ToolKind::Codex).expect("args");
    assert_eq!(
        args,
        " -c 'mcp_servers.brain.command=\"npx\"' -c 'mcp_servers.brain.args=[\"-y\", \"b\"]' -c 'mcp_servers.brain.env={ K = \"v\" }'"
    );
}

#[test]
fn codex_gets_skills_through_an_overlay_home_that_links_back_to_the_real_one() {
    let (lib, dir) = library("codex-home");
    let home = dir.join("home");
    for rel in [
        ".codex/sessions/2026",
        ".codex/skills/mine",
        ".codex/skills/tidy",
        ".codex/skills/.system",
    ] {
        fs::create_dir_all(home.join(rel)).expect("mkdir");
    }
    fs::write(home.join(".codex/auth.json"), "{}").expect("write");
    skill(&lib, "tidy", "name: tidy");
    skill(&lib, "fresh", "name: fresh");
    let l = lib.launch(ToolKind::Codex, &home, None).expect("launch");
    let overlay = dir.join("build/codex-home");
    assert_eq!(
        l.env,
        vec![("CODEX_HOME", overlay.to_string_lossy().into_owned())]
    );
    // Everything but skills/ is the real folder's own entry.
    assert_eq!(
        fs::read_link(overlay.join("sessions")).expect("link"),
        home.join(".codex/sessions")
    );
    assert_eq!(
        fs::read_link(overlay.join("auth.json")).expect("link"),
        home.join(".codex/auth.json")
    );
    assert!(
        !fs::symlink_metadata(overlay.join("skills"))
            .expect("skills")
            .file_type()
            .is_symlink()
    );
    let skills = overlay.join("skills");
    assert_eq!(
        fs::read_link(skills.join("mine")).expect("link"),
        home.join(".codex/skills/mine")
    );
    assert_eq!(
        fs::read_link(skills.join(".system")).expect("link"),
        home.join(".codex/skills/.system")
    );
    // The user's own tidy wins over the building's.
    assert_eq!(
        fs::read_link(skills.join("tidy")).expect("link"),
        home.join(".codex/skills/tidy")
    );
    assert_eq!(
        fs::read_link(skills.join("fresh")).expect("link"),
        lib.dir().join("skills/fresh")
    );
}

#[test]
fn opencode_gets_a_config_folder_of_skills_and_its_servers_inline() {
    let (lib, dir) = library("opencode");
    skill(&lib, "tidy", "name: tidy");
    manifest(
        &lib,
        r#"{"mcpServers":{"brain":{"command":"npx","args":["-y","b"],"env":{"K":"v"}}}}"#,
    );
    let l = lib
        .launch(ToolKind::Opencode, Path::new("/nowhere"), None)
        .expect("launch");
    let folder = dir.join("build/opencode");
    assert_eq!(
        l.env,
        vec![
            ("OPENCODE_CONFIG_DIR", folder.to_string_lossy().into_owned()),
            (
                "OPENCODE_CONFIG_CONTENT",
                r#"{"mcp":{"brain":{"type":"local","command":["npx","-y","b"],"environment":{"K":"v"}}}}"#.to_string()
            ),
        ]
    );
    assert!(l.opts.is_empty());
    assert_eq!(
        fs::read_link(folder.join("skills/tidy")).expect("link"),
        lib.dir().join("skills/tidy")
    );
    assert!(l.prefix(&quote).starts_with("env OPENCODE_CONFIG_DIR='"));
}

#[test]
fn tools_that_cannot_take_an_item_get_nothing() {
    let (lib, _) = library("none");
    manifest(&lib, r#"{"mcpServers":{"brain":{"command":"npx"}}}"#);
    // Cursor takes skills only, and there are none.
    assert_eq!(launch(&lib, ToolKind::Cursor).expect("args"), "");
    skill(&lib, "tidy", "name: tidy");
    // Antigravity takes the skill but not the server.
    let agy = lib
        .launch(ToolKind::Antigravity, Path::new("/nowhere"), None)
        .expect("launch");
    assert_eq!(agy.opts.len(), 1);
    assert_eq!(agy.opts[0].flag, "--add-dir");
    let cursor = lib
        .launch(ToolKind::Cursor, Path::new("/nowhere"), None)
        .expect("launch");
    assert_eq!(cursor.opts[0].flag, "--plugin-dir");
}

#[test]
fn the_inventory_reads_each_harness_from_its_own_files() {
    let (_, dir) = library("inventory");
    let home = dir.join("home");
    let write = |rel: &str, text: &str| {
        let path = home.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    };
    write(
        ".claude/skills/humanizer/SKILL.md",
        "---\nname: humanizer\ndescription: Edits\n---\n",
    );
    write(
        ".claude.json",
        r#"{"mcpServers":{"brain":{"type":"http"}},"other":1}"#,
    );
    write(
        ".claude/settings.json",
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"a.sh"},{"type":"prompt"},{"type":"command","command":"b.sh"}]}]},"enabledPlugins":{"a@b":true,"c@d":false}}"#,
    );
    write(".agents/skills/shared/SKILL.md", "---\nname: shared\n---\n");
    write(".codex/skills/shared/SKILL.md", "---\nname: shared\n---\n");
    write(".codex/skills/.system/SKILL.md", "---\nname: hidden\n---\n");
    write(
        ".codex/config.toml",
        "model = \"x\"\n[mcp_servers.docs]\ncommand = \"d\"\n[plugins.\"p@q\"]\nenabled = false\n",
    );
    write(
        ".codex/hooks.json",
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"start.sh"}]}]}}"#,
    );
    let found = inventory::harnesses(&home, &[ToolKind::ClaudeCode]);
    let claude = &found[0];
    assert!(claude.installed);
    assert_eq!(
        claude.skills.as_ref().expect("skills")[0].description,
        "Edits"
    );
    assert_eq!(claude.mcp_servers, Some(vec!["brain".to_string()]));
    let hooks = claude.hooks.as_ref().expect("hooks");
    assert_eq!(hooks.len(), 2, "prompt hooks are not commands");
    assert_eq!(
        (hooks[1].id.as_str(), hooks[1].command.as_str()),
        ("Stop:1", "b.sh")
    );
    assert_eq!(hooks[1].matcher.as_deref(), Some("*"));
    assert_eq!(
        claude.instructions,
        Some(Vec::new()),
        "no CLAUDE.md in this home"
    );
    assert_eq!(claude.plugins.as_ref().expect("plugins").len(), 2);
    let codex = &found[1];
    assert!(!codex.installed);
    assert_eq!(codex.skills.as_ref().expect("skills").len(), 1);
    assert_eq!(codex.mcp_servers, Some(vec!["docs".to_string()]));
    assert!(!codex.plugins.as_ref().expect("plugins")[0].enabled);
    assert_eq!(
        codex.hooks.as_ref().expect("hooks")[0].event,
        "SessionStart"
    );
    assert_eq!(found[2].hooks, None);
    assert_eq!(found[4].tool, ToolKind::Cursor);
    assert_eq!(
        found[4].skills.as_ref().map(Vec::len),
        Some(1),
        "~/.agents/skills is Cursor's too"
    );
}

#[test]
fn opencode_config_with_comments_is_read() {
    let (_, dir) = library("opencode-inventory");
    let home = dir.join("home");
    let conf = home.join(".config/opencode");
    fs::create_dir_all(conf.join("skills/tidy")).expect("mkdir");
    fs::write(conf.join("skills/tidy/SKILL.md"), "---\nname: tidy\n---\n").expect("write");
    fs::write(
        conf.join("opencode.jsonc"),
        "{\n  // servers\n  \"mcp\": { \"docs\": { \"type\": \"local\", \"command\": [\"a//b\"] } }, /* plugins */\n  \"plugin\": [\"p\"]\n}\n",
    )
    .expect("write");
    let found = inventory::harnesses(&home, &[]);
    let oc = found
        .iter()
        .find(|h| h.tool == ToolKind::Opencode)
        .expect("opencode");
    assert_eq!(oc.skills.as_ref().expect("skills").len(), 1);
    assert_eq!(oc.mcp_servers, Some(vec!["docs".to_string()]));
    assert_eq!(oc.plugins.as_ref().expect("plugins")[0].name, "p");
}

#[test]
fn a_tools_skill_is_copied_in_and_aimed_at_the_other_tools() {
    let (lib, dir) = library("import-skill");
    let home = dir.join("home");
    // The tool's skill is itself a link, as installers often make them.
    let real = dir.join("elsewhere/humanizer");
    fs::create_dir_all(real.join("refs")).expect("mkdir");
    fs::write(
        real.join("SKILL.md"),
        "---\nname: humanizer\ndescription: Edits\n---\n",
    )
    .expect("write");
    fs::write(real.join("refs/a.md"), "ref").expect("write");
    fs::create_dir_all(home.join(".claude/skills")).expect("mkdir");
    std::os::unix::fs::symlink(&real, home.join(".claude/skills/humanizer")).expect("link");
    lib.import(&home, ToolKind::ClaudeCode, ItemKind::Skill, "humanizer")
        .expect("import");
    let copy = lib.dir().join("skills/humanizer");
    assert!(
        !fs::symlink_metadata(&copy)
            .expect("copy")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(copy.join("refs/a.md")).expect("read"),
        "ref"
    );
    let view = lib.view(Path::new("/nowhere"), &[]);
    let Targets::Only { tools } = &view.items[0].targets else {
        panic!("expected chosen tools");
    };
    assert!(!tools.contains(&ToolKind::ClaudeCode));
    assert!(tools.contains(&ToolKind::Codex) && tools.contains(&ToolKind::Antigravity));
    assert!(
        lib.import(&home, ToolKind::ClaudeCode, ItemKind::Skill, "humanizer")
            .is_err()
    );
    assert!(
        lib.import(&home, ToolKind::ClaudeCode, ItemKind::Skill, "../etc")
            .is_err()
    );
}

#[test]
fn command_and_http_servers_are_copied_in_and_an_sse_one_is_refused() {
    let (lib, dir) = library("import-mcp");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("mkdir");
    fs::write(
        home.join(".claude.json"),
        r#"{"mcpServers":{"docs":{"command":"npx","args":["-y","d"],"env":{"K":"v"}},
            "web":{"type":"http","url":"https://x/mcp","headers":{"Authorization":"Bearer t"}},
            "old":{"type":"sse","url":"https://y/sse"}}}"#,
    )
    .expect("write");
    for name in ["docs", "web"] {
        lib.import(&home, ToolKind::ClaudeCode, ItemKind::McpServer, name)
            .expect("import");
    }
    let view = lib.view(Path::new("/nowhere"), &[]);
    let described: Vec<(&str, &str)> = view
        .items
        .iter()
        .map(|i| (i.name.as_str(), i.description.as_str()))
        .collect();
    assert_eq!(described, [("docs", "npx"), ("web", "https://x/mcp")]);
    let saved = fs::read_to_string(lib.dir().join("setup.json")).expect("read");
    assert!(saved.contains(r#""Authorization": "Bearer t""#));
    let err = lib
        .import(&home, ToolKind::ClaudeCode, ItemKind::McpServer, "old")
        .expect_err("sse");
    assert!(err.contains("streamable HTTP"));
}

#[test]
fn codex_and_opencode_http_servers_are_copied_in() {
    let (lib, dir) = library("import-http");
    let home = dir.join("home");
    fs::create_dir_all(home.join(".codex")).expect("mkdir");
    fs::create_dir_all(home.join(".config/opencode")).expect("mkdir");
    fs::write(
        home.join(".codex/config.toml"),
        "[mcp_servers.brain]\nurl = \"http://h:1/mcp\"\nhttp_headers = { Authorization = \"Bearer t\" }\n\n[mcp_servers.env_auth]\nurl = \"https://e\"\nbearer_token_env_var = \"TOKEN\"\n",
    )
    .expect("write");
    fs::write(
        home.join(".config/opencode/opencode.json"),
        r#"{"mcp":{"far":{"type":"remote","url":"https://far/mcp","headers":{"X":"y"}}}}"#,
    )
    .expect("write");
    lib.import(&home, ToolKind::Codex, ItemKind::McpServer, "brain")
        .expect("codex");
    lib.import(&home, ToolKind::Opencode, ItemKind::McpServer, "far")
        .expect("opencode");
    assert!(
        lib.import(&home, ToolKind::Codex, ItemKind::McpServer, "env_auth")
            .is_err()
    );
    let saved = fs::read_to_string(lib.dir().join("setup.json")).expect("read");
    assert!(saved.contains(r#""url": "http://h:1/mcp""#) && saved.contains(r#""X": "y""#));
}

#[test]
fn each_tool_gets_an_http_server_in_its_own_form() {
    let (lib, dir) = library("http");
    manifest(
        &lib,
        r#"{"mcpServers":{"brain":{"url":"http://h:1/mcp","headers":{"Authorization":"Bearer t"}}}}"#,
    );
    assert_eq!(
        launch(&lib, ToolKind::Codex).expect("codex"),
        " -c 'mcp_servers.brain.url=\"http://h:1/mcp\"' -c 'mcp_servers.brain.http_headers={ Authorization = \"Bearer t\" }'"
    );
    launch(&lib, ToolKind::ClaudeCode).expect("claude");
    let mcp = fs::read_to_string(dir.join("build/claude-code/.mcp.json")).expect("mcp");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&mcp).expect("json"),
        serde_json::json!({"mcpServers":{"brain":{"type":"http","url":"http://h:1/mcp","headers":{"Authorization":"Bearer t"}}}})
    );
    let l = lib
        .launch(ToolKind::Opencode, Path::new("/nowhere"), None)
        .expect("opencode");
    assert_eq!(
        l.env,
        vec![(
            "OPENCODE_CONFIG_CONTENT",
            r#"{"mcp":{"brain":{"type":"remote","url":"http://h:1/mcp","headers":{"Authorization":"Bearer t"}}}}"#.to_string()
        )]
    );
}

fn instructions(lib: &Library, name: &str, text: &str) {
    fs::write(
        lib.dir().join("instructions").join(format!("{name}.md")),
        text,
    )
    .expect("write");
}

#[test]
fn the_view_lists_every_kind_of_item_with_a_description() {
    let (lib, _) = library("kinds");
    skill(&lib, "tidy", "name: tidy\ndescription: Tidies");
    instructions(&lib, "house", "# House rules\n\nNo semicolons.\n");
    manifest(
        &lib,
        r#"{"mcpServers":{"docs":{"command":"npx"}},
            "hooks":{"fmt":{"event":"PostToolUse","matcher":"Edit","command":"./fmt.sh"}}}"#,
    );
    let view = lib.view(Path::new("/nowhere"), &[]);
    let got: Vec<(ItemKind, &str, &str)> = view
        .items
        .iter()
        .map(|i| (i.kind, i.name.as_str(), i.description.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            (ItemKind::Skill, "tidy", "Tidies"),
            (ItemKind::Instructions, "house", "House rules"),
            (ItemKind::McpServer, "docs", "npx"),
            (ItemKind::Hook, "fmt", "PostToolUse (Edit): ./fmt.sh"),
        ]
    );
    lib.set_targets(ItemKind::Hook, "fmt", Targets::Only { tools: vec![] })
        .expect("hook");
    lib.set_targets(
        ItemKind::Instructions,
        "house",
        Targets::Only { tools: vec![] },
    )
    .expect("instructions");
    assert!(
        lib.set_targets(ItemKind::Instructions, "nope", Targets::All)
            .is_err()
    );
}

#[test]
fn claude_code_gets_hooks_in_its_plugin_and_instructions_as_a_system_prompt_file() {
    let (lib, dir) = library("claude-hooks");
    instructions(&lib, "a", "First.");
    instructions(&lib, "b", "Second.");
    manifest(
        &lib,
        r#"{"hooks":{"fmt":{"event":"PostToolUse","matcher":"Edit","command":"./fmt.sh"}}}"#,
    );
    let l = lib
        .launch(ToolKind::ClaudeCode, Path::new("/nowhere"), None)
        .expect("launch");
    let flags: Vec<&str> = l.opts.iter().map(|o| o.flag).collect();
    assert_eq!(flags, vec!["--plugin-dir", "--append-system-prompt-file"]);
    let hooks = fs::read_to_string(dir.join("build/claude-code/hooks/hooks.json")).expect("hooks");
    let parsed: serde_json::Value = serde_json::from_str(&hooks).expect("json");
    assert_eq!(parsed["hooks"]["PostToolUse"][0]["matcher"], "Edit");
    assert_eq!(
        parsed["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
        "./fmt.sh"
    );
    assert_eq!(
        fs::read_to_string(dir.join("build/claude-code-instructions.md")).expect("text"),
        "First.\n\nSecond.\n"
    );
}

#[test]
fn codex_overlay_adds_the_buildings_instructions_and_hooks_to_the_users_own() {
    let (lib, dir) = library("codex-extra");
    let home = dir.join("home");
    fs::create_dir_all(home.join(".codex")).expect("mkdir");
    fs::write(home.join(".codex/AGENTS.md"), "Mine.\n").expect("write");
    fs::write(
        home.join(".codex/hooks.json"),
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"mine.sh","timeout":5}]}],"Notification":[{"x":1}]}}"#,
    )
    .expect("write");
    instructions(&lib, "house", "Ours.");
    manifest(
        &lib,
        r#"{"hooks":{"fmt":{"event":"Stop","command":"ours.sh"}}}"#,
    );
    lib.launch(ToolKind::Codex, &home, None).expect("launch");
    let overlay = dir.join("build/codex-home");
    assert_eq!(
        fs::read_to_string(overlay.join("AGENTS.md")).expect("agents"),
        "Mine.\n\nOurs.\n"
    );
    let hooks: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(overlay.join("hooks.json")).expect("hooks"))
            .expect("json");
    assert_eq!(hooks["hooks"]["Stop"][0]["hooks"][0]["timeout"], 5);
    assert_eq!(hooks["hooks"]["Stop"][1]["hooks"][0]["command"], "ours.sh");
    assert_eq!(hooks["hooks"]["Notification"][0]["x"], 1);
    // With only a skill aimed at Codex, the files link back to the user's own.
    manifest(
        &lib,
        r#"{"instructions":{"house":{"targets":{"kind":"only","tools":[]}}},"skills":{}}"#,
    );
    skill(&lib, "tidy", "name: tidy");
    lib.launch(ToolKind::Codex, &home, None).expect("launch");
    assert_eq!(
        fs::read_link(overlay.join("AGENTS.md")).expect("link"),
        home.join(".codex/AGENTS.md")
    );
    assert_eq!(
        fs::read_link(overlay.join("hooks.json")).expect("link"),
        home.join(".codex/hooks.json")
    );
}

#[test]
fn opencode_and_antigravity_get_instructions_their_own_way() {
    let (lib, dir) = library("instructions");
    instructions(&lib, "house", "Ours.");
    let oc = lib
        .launch(ToolKind::Opencode, Path::new("/nowhere"), None)
        .expect("opencode");
    let file = lib.dir().join("instructions/house.md");
    assert_eq!(
        oc.env,
        vec![(
            "OPENCODE_CONFIG_CONTENT",
            format!(r#"{{"instructions":["{}"]}}"#, file.display())
        )]
    );
    let agy = lib
        .launch(ToolKind::Antigravity, Path::new("/nowhere"), None)
        .expect("agy");
    assert_eq!(agy.opts[0].flag, "--add-dir");
    assert_eq!(
        fs::read_to_string(dir.join("build/antigravity/AGENTS.md")).expect("agents"),
        "Ours.\n"
    );
}

#[test]
fn a_tools_instructions_and_hooks_are_copied_in_for_the_other_tools() {
    let (lib, dir) = library("import-more");
    let home = dir.join("home");
    fs::create_dir_all(home.join(".claude")).expect("mkdir");
    fs::write(home.join(".claude/CLAUDE.md"), "# Mine\nBritish English.\n").expect("write");
    fs::write(
        home.join(".claude/settings.json"),
        r#"{"hooks":{"PostToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":"fmt.sh"}]}],"Unknown":[{"hooks":[{"type":"command","command":"x"}]}]}}"#,
    )
    .expect("write");
    let found = inventory::harnesses(&home, &[]);
    assert_eq!(found[0].instructions, Some(vec!["CLAUDE.md".to_string()]));
    lib.import(
        &home,
        ToolKind::ClaudeCode,
        ItemKind::Instructions,
        "CLAUDE.md",
    )
    .expect("instructions");
    lib.import(&home, ToolKind::ClaudeCode, ItemKind::Hook, "PostToolUse:0")
        .expect("hook");
    let err = lib
        .import(&home, ToolKind::ClaudeCode, ItemKind::Hook, "Unknown:0")
        .expect_err("unknown event");
    assert!(err.contains("not an event"));
    let view = lib.view(Path::new("/nowhere"), &[]);
    let names: Vec<(&str, ItemKind)> = view
        .items
        .iter()
        .map(|i| (i.name.as_str(), i.kind))
        .collect();
    assert_eq!(
        names,
        vec![
            ("claude-code-instructions", ItemKind::Instructions),
            ("claude-code-posttooluse-0", ItemKind::Hook),
        ]
    );
    assert_eq!(view.items[0].description, "Mine");
    assert_eq!(view.items[1].description, "PostToolUse (Edit): fmt.sh");
    let Targets::Only { tools } = &view.items[1].targets else {
        panic!("expected chosen tools");
    };
    assert_eq!(
        tools,
        &vec![ToolKind::Codex],
        "only Codex takes hooks besides Claude Code"
    );
}
