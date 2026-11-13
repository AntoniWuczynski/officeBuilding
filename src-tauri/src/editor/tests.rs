use super::*;
use crate::process::{Captured, FakeRunner};
use std::ffi::OsString;

const SHELL: &str = "/bin/test-shell";
const CURSOR: &str = "/nonexistent-ob/bin/cursor";
const CODE: &str = "/nonexistent-ob/bin/code";
const FOLDER: &str = "/nonexistent-ob/a b/it's \"here\"";

/// A scratch folder standing in for the Applications folders, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ob-editor-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        Self(dir)
    }

    fn apps(&self) -> Vec<PathBuf> {
        vec![self.0.join("Apps")]
    }

    fn app(&self, name: &str) -> PathBuf {
        let bundle = self.0.join("Apps").join(format!("{name}.app"));
        fs::create_dir_all(&bundle).expect("bundle");
        bundle
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn is_probe(call: &Call) -> bool {
    call.args.first().is_some_and(|a| a == "-l")
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "it is the reply a FakeRunner closure returns"
)]
fn exited(code: Option<i32>) -> Result<Captured, String> {
    Ok(Captured {
        code,
        ..Captured::default()
    })
}

/// A runner whose probe prints `probe` (exiting 1, as a missing last name does) and whose
/// launches answer with `launch`.
fn fake(
    probe: &str,
    launch: impl Fn(&Call) -> Result<Captured, String> + Send + Sync + 'static,
) -> Arc<FakeRunner> {
    let probe = probe.to_string();
    Arc::new(FakeRunner::new(move |call| {
        if is_probe(call) {
            Ok(Captured {
                code: Some(1),
                stdout: probe.clone(),
                stderr: String::new(),
            })
        } else {
            launch(call)
        }
    }))
}

fn found(probe: &str, app_dirs: &[PathBuf]) -> Vec<Installed> {
    detect(fake(probe, |_| exited(Some(0))).as_ref(), SHELL, app_dirs)
}

fn editors_on(fake: &Arc<FakeRunner>, app_dirs: Vec<PathBuf>) -> Editors {
    let runner: Arc<dyn Runner> = fake.clone();
    Editors::new(runner, app_dirs)
}

fn probes(fake: &FakeRunner) -> usize {
    fake.calls().iter().filter(|c| is_probe(c)).count()
}

fn launches(fake: &FakeRunner) -> Vec<Call> {
    fake.calls().into_iter().filter(|c| !is_probe(c)).collect()
}

fn command(path: &str) -> Route {
    Route::Command(PathBuf::from(path))
}

#[test]
fn the_probe_is_one_login_shell_run_built_from_the_command_names() {
    let runner = fake("", |_| exited(Some(0)));
    detect(runner.as_ref(), SHELL, &[]);
    let want = Call::new(
        SHELL,
        [
            "-l",
            "-c",
            "command -v cursor; command -v code; command -v zed; command -v devin-desktop; command -v windsurf",
        ],
    );
    assert_eq!(runner.calls(), vec![want]);
}

#[test]
fn a_command_alone_is_a_command_route() {
    assert_eq!(
        found(&format!("{CODE}\n"), &[]),
        vec![Installed {
            editor: Editor::VsCode,
            routes: vec![command(CODE)],
        }]
    );
}

#[test]
fn an_app_alone_is_an_app_route() {
    let tmp = Temp::new("app-alone");
    let bundle = tmp.app("Cursor");
    assert_eq!(
        found("", &tmp.apps()),
        vec![Installed {
            editor: Editor::Cursor,
            routes: vec![Route::App(bundle)],
        }]
    );
}

#[test]
fn the_command_comes_before_the_app() {
    let tmp = Temp::new("both");
    let bundle = tmp.app("Cursor");
    let got = found(&format!("{CURSOR}\n"), &tmp.apps());
    assert_eq!(
        got,
        vec![Installed {
            editor: Editor::Cursor,
            routes: vec![command(CURSOR), Route::App(bundle)],
        }]
    );
}

#[test]
fn lines_that_are_not_a_listed_command_path_are_ignored() {
    let probe = "Welcome to the shell!\nalias code='open -a x'\ncode\n  \n/usr/bin/codex\n/opt/zed/zed-cli\n";
    assert_eq!(found(probe, &[]), Vec::new());
}

#[test]
fn devin_desktop_is_found_by_either_name_and_the_new_one_wins() {
    let windsurf = "/nonexistent-ob/bin/windsurf";
    let devin = "/nonexistent-ob/bin/devin-desktop";
    let want = |path: &str| {
        vec![Installed {
            editor: Editor::DevinDesktop,
            routes: vec![command(path)],
        }]
    };
    assert_eq!(found(&format!("{windsurf}\n"), &[]), want(windsurf));
    assert_eq!(found(&format!("{windsurf}\n{devin}\n"), &[]), want(devin));

    let tmp = Temp::new("windsurf-app");
    let bundle = tmp.app("Windsurf");
    assert_eq!(
        found("", &tmp.apps()),
        vec![Installed {
            editor: Editor::DevinDesktop,
            routes: vec![Route::App(bundle)],
        }]
    );
}

#[test]
fn a_command_inside_another_listed_editors_bundle_does_not_count() {
    let tmp = Temp::new("bundle");
    let inner = tmp.0.join("Apps/Cursor.app/Contents/Resources/app/bin");
    let other = tmp.0.join("Apps/Some Fork.app/Contents/bin");
    fs::create_dir_all(&inner).expect("inner");
    fs::create_dir_all(&other).expect("other");
    fs::write(inner.join("code"), "").expect("code");
    fs::write(other.join("zed"), "").expect("zed");
    let bin = tmp.0.join("bin");
    fs::create_dir_all(&bin).expect("bin");
    std::os::unix::fs::symlink(inner.join("code"), bin.join("code")).expect("link code");
    std::os::unix::fs::symlink(inner.join("code"), bin.join("cursor")).expect("link cursor");
    std::os::unix::fs::symlink(other.join("zed"), bin.join("zed")).expect("link zed");
    let probe = [bin.join("code"), bin.join("cursor"), bin.join("zed")]
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    // No app folders, so only command routes can be found.
    let got = found(&probe, &[]);
    assert_eq!(
        got,
        vec![
            Installed {
                editor: Editor::Cursor,
                routes: vec![Route::Command(bin.join("cursor"))],
            },
            Installed {
                editor: Editor::Zed,
                routes: vec![Route::Command(bin.join("zed"))],
            },
        ]
    );
}

#[test]
fn a_probe_that_fails_still_finds_the_apps() {
    let tmp = Temp::new("probe-fails");
    let bundle = tmp.app("Zed");
    let want = vec![Installed {
        editor: Editor::Zed,
        routes: vec![Route::App(bundle)],
    }];
    let refused = FakeRunner::new(|_| Err("/bin/test-shell could not be started: no".into()));
    assert_eq!(detect(&refused, SHELL, &tmp.apps()), want);
    let silent = FakeRunner::new(|_| exited(Some(127)));
    assert_eq!(detect(&silent, SHELL, &tmp.apps()), want);
}

#[test]
fn list_gives_all_four_in_order_with_the_default() {
    let tmp = Temp::new("list");
    tmp.app("Zed");
    let runner = fake(&format!("{CODE}\n"), |_| exited(Some(0)));
    let editors = editors_on(&runner, tmp.apps());
    let list = editors.list(None);
    let rows: Vec<(Editor, &str, bool)> = list
        .editors
        .iter()
        .map(|o| (o.editor, o.label, o.installed))
        .collect();
    assert_eq!(
        rows,
        vec![
            (Editor::Cursor, "Cursor", false),
            (Editor::VsCode, "VS Code", true),
            (Editor::Zed, "Zed", true),
            (Editor::DevinDesktop, "Devin Desktop", false),
        ]
    );
    assert_eq!(list.default_editor, Some(Editor::VsCode));
    assert_eq!(
        editors.list(Some(Editor::Zed)).default_editor,
        Some(Editor::Zed)
    );
    assert_eq!(
        editors.list(Some(Editor::Cursor)).default_editor,
        Some(Editor::VsCode)
    );

    let nothing = fake("", |_| exited(Some(0)));
    let list = editors_on(&nothing, Vec::new()).list(Some(Editor::Zed));
    assert_eq!(list.editors.len(), 4);
    assert!(list.editors.iter().all(|o| !o.installed));
    assert_eq!(list.default_editor, None);
}

#[test]
fn open_passes_the_folder_as_one_argument_on_each_route() {
    let tmp = Temp::new("argv");
    let bundle = tmp.app("Cursor");
    // The command fails, so both routes run.
    let runner = fake(&format!("{CURSOR}\n"), |call| {
        exited(Some(i32::from(call.program.as_path() == Path::new(CURSOR))))
    });
    let editors = editors_on(&runner, tmp.apps());
    assert_eq!(
        editors.open(Path::new(FOLDER), None, None),
        Ok(Editor::Cursor)
    );
    assert_eq!(
        launches(&runner),
        vec![
            Call::new(CURSOR, [FOLDER]),
            Call::new(
                "/usr/bin/open",
                [OsString::from("-a"), bundle.into_os_string(), FOLDER.into()],
            ),
        ]
    );
}

#[test]
fn open_counts_exit_zero_and_still_running_as_opened() {
    for code in [Some(0), None] {
        let runner = fake(&format!("{CODE}\n"), move |_| exited(code));
        let editors = editors_on(&runner, Vec::new());
        assert_eq!(
            editors.open(Path::new(FOLDER), Some(Editor::VsCode), None),
            Ok(Editor::VsCode),
            "{code:?}"
        );
        assert_eq!(launches(&runner).len(), 1);
    }
}

#[test]
fn open_falls_back_to_the_app_and_reports_when_every_route_fails() {
    let tmp = Temp::new("fallback");
    tmp.app("Cursor");
    let refuse = |call: &Call| {
        if call.program.as_path() == Path::new(CURSOR) {
            Err(format!("{CURSOR} could not be started: gone"))
        } else {
            exited(Some(0))
        }
    };
    let runner = fake(&format!("{CURSOR}\n"), refuse);
    let editors = editors_on(&runner, tmp.apps());
    assert_eq!(
        editors.open(Path::new(FOLDER), None, None),
        Ok(Editor::Cursor)
    );
    assert_eq!(launches(&runner).len(), 2);

    // Exit 1 on the command falls back to the app, which exits 1 too.
    let runner = fake(&format!("{CURSOR}\n"), |_| exited(Some(1)));
    let editors = editors_on(&runner, tmp.apps());
    assert_eq!(
        editors.open(Path::new(FOLDER), None, None),
        Err("Cursor did not open: it stopped with exit code 1".into())
    );
    assert_eq!(launches(&runner).len(), 2);

    let runner = fake(&format!("{CURSOR}\n"), |_| {
        Err("/usr/bin/open could not be started: denied".into())
    });
    let editors = editors_on(&runner, tmp.apps());
    assert_eq!(
        editors.open(Path::new(FOLDER), None, None),
        Err("Cursor did not open: /usr/bin/open could not be started: denied".into())
    );
}

#[test]
fn open_refuses_a_missing_editor_no_editor_and_a_relative_folder() {
    let runner = fake(&format!("{CODE}\n"), |_| exited(Some(0)));
    let editors = editors_on(&runner, Vec::new());
    assert_eq!(
        editors.open(Path::new(FOLDER), Some(Editor::Zed), None),
        Err("Zed is not installed on this Mac".into())
    );
    assert_eq!(
        editors.open(Path::new("relative/folder"), None, None),
        Err("that folder's path is not a full path".into())
    );
    assert_eq!(
        editors.open(Path::new("-n"), None, None),
        Err("that folder's path is not a full path".into())
    );

    let nothing = fake("", |_| exited(Some(0)));
    assert_eq!(
        editors_on(&nothing, Vec::new()).open(Path::new(FOLDER), None, Some(Editor::Zed)),
        Err(
            "no editor was found on this Mac (looked for Cursor, VS Code, Zed and Devin Desktop)"
                .into()
        )
    );
    assert!(launches(&runner).is_empty());
    assert!(launches(&nothing).is_empty());
}

#[test]
fn open_uses_the_remembered_detection_and_forgets_it_after_a_failure() {
    let runner = fake(&format!("{CODE}\n"), |_| exited(Some(0)));
    let editors = editors_on(&runner, Vec::new());
    editors.list(None);
    assert_eq!(probes(&runner), 1);
    assert!(editors.open(Path::new(FOLDER), None, None).is_ok());
    assert_eq!(probes(&runner), 1);

    let runner = fake(&format!("{CODE}\n"), |_| exited(Some(2)));
    let editors = editors_on(&runner, Vec::new());
    assert!(editors.open(Path::new(FOLDER), None, None).is_err());
    assert_eq!(probes(&runner), 1);
    assert!(editors.open(Path::new(FOLDER), None, None).is_err());
    assert_eq!(probes(&runner), 2);
}

#[test]
fn open_prefers_the_preferred_editor_when_none_is_asked() {
    let tmp = Temp::new("preferred");
    tmp.app("Zed");
    let runner = fake(&format!("{CODE}\n"), |_| exited(Some(0)));
    let editors = editors_on(&runner, tmp.apps());
    assert_eq!(
        editors.open(Path::new(FOLDER), None, Some(Editor::Zed)),
        Ok(Editor::Zed)
    );
    assert_eq!(
        editors.open(Path::new(FOLDER), None, Some(Editor::Cursor)),
        Ok(Editor::VsCode)
    );
}

#[test]
fn editor_serialises_to_the_kebab_case_names() {
    let names: Vec<String> = [
        Editor::Cursor,
        Editor::VsCode,
        Editor::Zed,
        Editor::DevinDesktop,
    ]
    .iter()
    .map(|e| serde_json::to_string(e).expect("json"))
    .collect();
    assert_eq!(
        names,
        ["\"cursor\"", "\"vs-code\"", "\"zed\"", "\"devin-desktop\""]
    );
    assert!(serde_json::from_str::<Editor>("\"vscode\"").is_err());
}

#[test]
fn every_editor_maps_to_its_own_spec() {
    for spec in &SPECS {
        assert_eq!(spec.editor.spec().label, spec.label);
    }
}
