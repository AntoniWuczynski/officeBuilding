//! Editors the building can open a folder in: which are installed, which is the default,
//! and the launch. The folder always comes from the backend (`Office::folder_of`).

use crate::process::{Call, Runner};
use crate::sync::lock;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a launch waits for the editor's command to hand over before counting it opened.
pub const LAUNCH_WAIT: Duration = Duration::from_secs(10);

/// An editor the building can open a folder in. Not `ToolKind`: `ToolKind::Cursor` is the
/// `cursor-agent` CLI, this `Cursor` is the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Editor {
    Cursor,
    VsCode,
    Zed,
    DevinDesktop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorOption {
    pub editor: Editor,
    pub label: &'static str,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorList {
    /// Every listed editor, always all four, in the order Cursor, VS Code, Zed, Devin Desktop.
    pub editors: Vec<EditorOption>,
    /// The one "Open in" uses: the one picked in Settings when it is installed, else the
    /// first installed. None: none is installed.
    pub default_editor: Option<Editor>,
}

struct Spec {
    editor: Editor,
    label: &'static str,
    /// Command names, newest first.
    commands: &'static [&'static str],
    /// App bundle names without `.app`, newest first.
    apps: &'static [&'static str],
}

const SPECS: [Spec; 4] = [
    Spec {
        editor: Editor::Cursor,
        label: "Cursor",
        commands: &["cursor"],
        apps: &["Cursor"],
    },
    Spec {
        editor: Editor::VsCode,
        label: "VS Code",
        commands: &["code"],
        apps: &["Visual Studio Code"],
    },
    Spec {
        editor: Editor::Zed,
        label: "Zed",
        commands: &["zed"],
        apps: &["Zed"],
    },
    Spec {
        editor: Editor::DevinDesktop,
        label: "Devin Desktop",
        commands: &["devin-desktop", "windsurf"],
        apps: &["Devin Desktop", "Windsurf"],
    },
];

impl Editor {
    fn spec(self) -> &'static Spec {
        match self {
            Self::Cursor => &SPECS[0],
            Self::VsCode => &SPECS[1],
            Self::Zed => &SPECS[2],
            Self::DevinDesktop => &SPECS[3],
        }
    }
}

/// One way to open a folder in an installed editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// The editor's own command, by the absolute path the login shell reported.
    Command(PathBuf),
    /// The app bundle, handed to `/usr/bin/open -a`.
    App(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub editor: Editor,
    /// In the order to try them: the command first, then the app.
    pub routes: Vec<Route>,
}

/// The probe script: built from the constant command names only, never from data.
fn probe_script() -> String {
    SPECS
        .iter()
        .flat_map(|s| s.commands)
        .map(|name| format!("command -v {name}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Whether `command` sits inside the app bundle of a listed editor other than `spec`'s.
/// Cursor ships a `code` command in its own bundle, which must not count as VS Code.
fn owned_by_another(command: &Path, spec: &Spec) -> bool {
    let real = fs::canonicalize(command).unwrap_or_else(|_| command.to_path_buf());
    real.ancestors()
        .filter_map(|dir| dir.file_name()?.to_str()?.strip_suffix(".app"))
        .any(|app| {
            SPECS
                .iter()
                .any(|other| other.editor != spec.editor && other.apps.contains(&app))
        })
}

fn command_route(spec: &Spec, paths: &[PathBuf]) -> Option<Route> {
    spec.commands.iter().find_map(|name| {
        paths
            .iter()
            .find(|p| p.file_name() == Some(OsStr::new(name)) && !owned_by_another(p, spec))
            .map(|p| Route::Command(p.clone()))
    })
}

fn app_route(spec: &Spec, app_dirs: &[PathBuf]) -> Option<Route> {
    spec.apps
        .iter()
        .flat_map(|app| {
            app_dirs
                .iter()
                .map(move |dir| dir.join(format!("{app}.app")))
        })
        .find(|bundle| bundle.is_dir())
        .map(Route::App)
}

/// The installed editors, in list order. One login-shell run plus a look in `app_dirs`.
pub fn detect(runner: &dyn Runner, shell: &str, app_dirs: &[PathBuf]) -> Vec<Installed> {
    let probe = Call::new(shell, ["-l".to_string(), "-c".to_string(), probe_script()]);
    // A probe that cannot start finds no commands. Its exit code says nothing: `command -v`
    // fails for the last name whenever that one is missing, whatever was found before it.
    let stdout = runner.capture(&probe).map(|c| c.stdout).unwrap_or_default();
    let paths: Vec<PathBuf> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('/'))
        .map(PathBuf::from)
        .collect();
    SPECS
        .iter()
        .filter_map(|spec| {
            let routes: Vec<Route> = [command_route(spec, &paths), app_route(spec, app_dirs)]
                .into_iter()
                .flatten()
                .collect();
            (!routes.is_empty()).then_some(Installed {
                editor: spec.editor,
                routes,
            })
        })
        .collect()
}

/// The editor to use: the one asked for, else the preferred one when installed, else the
/// first installed.
fn pick(
    found: &[Installed],
    asked: Option<Editor>,
    preferred: Option<Editor>,
) -> Result<&Installed, String> {
    let installed = |editor: Editor| found.iter().find(|i| i.editor == editor);
    if let Some(editor) = asked {
        return installed(editor)
            .ok_or_else(|| format!("{} is not installed on this Mac", editor.spec().label));
    }
    preferred
        .and_then(installed)
        .or_else(|| found.first())
        .ok_or_else(|| {
            let labels: Vec<&str> = SPECS.iter().map(|s| s.label).collect();
            let (last, rest) = labels.split_last().unwrap_or((&"", &[]));
            format!(
                "no editor was found on this Mac (looked for {} and {last})",
                rest.join(", ")
            )
        })
}

/// The program run for one route. The folder is one whole argument, with no window flag.
fn call(route: &Route, folder: &Path) -> Call {
    match route {
        Route::Command(command) => Call::new(command, [folder.as_os_str()]),
        Route::App(bundle) => Call::new(
            "/usr/bin/open",
            [OsStr::new("-a"), bundle.as_os_str(), folder.as_os_str()],
        ),
    }
}

/// Detection, remembered between calls, and the launch. Managed by Tauri as `Arc<Editors>`.
pub struct Editors {
    runner: Arc<dyn Runner>,
    app_dirs: Vec<PathBuf>,
    found: Mutex<Option<Vec<Installed>>>,
}

impl Editors {
    /// `app_dirs` in production: `/Applications` and `<home>/Applications`.
    #[must_use]
    pub fn new(runner: Arc<dyn Runner>, app_dirs: Vec<PathBuf>) -> Self {
        Self {
            runner,
            app_dirs,
            found: Mutex::new(None),
        }
    }

    /// Detect with no lock held, then remember the result.
    fn detect_now(&self) -> Vec<Installed> {
        let found = detect(
            self.runner.as_ref(),
            &crate::pty::login_shell(),
            &self.app_dirs,
        );
        *lock(&self.found) = Some(found.clone());
        found
    }

    /// Look afresh, remember what was found, and list every editor with the default.
    pub fn list(&self, preferred: Option<Editor>) -> EditorList {
        let found = self.detect_now();
        EditorList {
            editors: SPECS
                .iter()
                .map(|spec| EditorOption {
                    editor: spec.editor,
                    label: spec.label,
                    installed: found.iter().any(|i| i.editor == spec.editor),
                })
                .collect(),
            default_editor: pick(&found, None, preferred).ok().map(|i| i.editor),
        }
    }

    /// Open `folder` in `asked`, or in the default when None. Returns the editor used.
    pub fn open(
        &self,
        folder: &Path,
        asked: Option<Editor>,
        preferred: Option<Editor>,
    ) -> Result<Editor, String> {
        if !folder.is_absolute() {
            return Err("that folder's path is not a full path".into());
        }
        let cached = lock(&self.found).clone();
        let found = match cached {
            Some(found) => found,
            None => self.detect_now(),
        };
        let chosen = pick(&found, asked, preferred)?;
        let mut last = String::new();
        for route in &chosen.routes {
            match self.runner.launch(&call(route, folder), LAUNCH_WAIT) {
                Ok(Some(0) | None) => return Ok(chosen.editor),
                Ok(Some(code)) => last = format!("it stopped with exit code {code}"),
                Err(e) => last = e,
            }
        }
        *lock(&self.found) = None;
        Err(format!(
            "{} did not open: {last}",
            chosen.editor.spec().label
        ))
    }
}

#[cfg(test)]
mod tests;
