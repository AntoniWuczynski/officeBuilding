use std::path::Path;
use std::process::Command;

/// What a git command printed, or `None` when git is missing, fails or prints nothing.
fn git_output(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn main() {
    let commit = git_output(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=OB_GIT_COMMIT={commit}");
    // A checkout changes HEAD and a commit changes the ref HEAD names, so watch both. A branch
    // that was packed has no file of its own: its ref is in packed-refs, and cargo reruns a script
    // every build for a path that does not exist.
    for name in ["HEAD", "packed-refs"] {
        if let Some(path) = git_output(&["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = git_output(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git_output(&["rev-parse", "--git-path", &reference])
        && Path::new(&path).exists()
    {
        println!("cargo:rerun-if-changed={path}");
    }
    tauri_build::build();
}
