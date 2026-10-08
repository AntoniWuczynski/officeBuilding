//! Which `codex` processes are running and what each holds open: the liveness
//! half of Codex discovery (see the module doc of the parent).

use super::session_uuid;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Pids of the `codex` processes running right now, via `ps` (name match).
/// Never panics: an unavailable `ps` just yields none.
pub fn list_codex_pids() -> Vec<i32> {
    let Ok(out) = Command::new("ps").args(["-axo", "pid=,comm="]).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut result = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let mut parts = line.splitn(2, char::is_whitespace);
        let Some(pid_str) = parts.next() else {
            continue;
        };
        let Some(comm) = parts.next().map(str::trim_start) else {
            continue;
        };
        if comm.rsplit('/').next() != Some("codex") {
            continue;
        }
        let Ok(pid) = pid_str.parse::<i32>() else {
            continue;
        };
        result.push(pid);
    }
    result
}

/// A running `codex` process, its working directory, and the top-level
/// (non-sub-agent) rollout session ids `lsof` shows it holds open right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOpenRollouts {
    pub pid: i32,
    pub cwd: Option<String>,
    pub open_session_ids: Vec<String>,
}

/// A process is live for the rollout it holds open, when that is unambiguous:
/// exactly one top-level rollout open. Zero (no turn started yet: rollouts
/// are created lazily) or more than one is not enough evidence to pick one.
/// Pure and OS-call-free so the decision is directly testable.
#[must_use]
pub fn resolve_liveness_by_open_files(processes: &[ProcessOpenRollouts]) -> HashMap<String, i32> {
    processes
        .iter()
        .filter_map(|p| match p.open_session_ids.as_slice() {
            [only] => Some((only.clone(), p.pid)),
            _ => None,
        })
        .collect()
}

/// A process's cwd and open files out of `lsof -Ffn` output, where an `fcwd`
/// line is followed by the `n<path>` line naming the directory.
fn parse_lsof(text: &str) -> (Option<String>, Vec<&str>) {
    let mut cwd = None;
    let mut files = Vec::new();
    let mut on_cwd = false;
    for line in text.lines() {
        if let Some(fd) = line.strip_prefix('f') {
            on_cwd = fd == "cwd";
        } else if let Some(path) = line.strip_prefix('n') {
            if on_cwd {
                cwd = Some(path.to_string());
            } else {
                files.push(path);
            }
        }
    }
    (cwd, files)
}

/// The cwd and top-level rollout session ids a process holds open, via one
/// `lsof -p <pid> -Ffn` call, filtered to files under `sessions_dir` matching
/// `rollout-*.jsonl` and further filtered to `top_level` (rollouts already
/// known, from the parse pass, not to be sub-agent threads). Never panics: an
/// unavailable `lsof`, or a process that exits mid-lookup, just yields none.
pub fn probe(pid: i32, sessions_dir: &Path, top_level: &HashSet<String>) -> ProcessOpenRollouts {
    let text = Command::new("lsof")
        .args(["-p", &pid.to_string(), "-Ffn"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();
    let (cwd, files) = parse_lsof(&text);
    let open_session_ids = files
        .into_iter()
        .map(Path::new)
        .filter(|p| p.starts_with(sessions_dir))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .filter(|stem| stem.starts_with("rollout-"))
        .filter_map(session_uuid)
        .filter(|id| top_level.contains(id))
        .collect();
    ProcessOpenRollouts {
        pid,
        cwd,
        open_session_ids,
    }
}

/// Whether a running `codex` is an interactive session of its own, from its
/// cwd and command line. The `ChatGPT` desktop app's embedded codex has cwd
/// `/`; IDE extensions and the desktop app run `app-server` / `mcp-server`
/// from a workspace cwd. None of those is a desk.
#[must_use]
pub fn is_interactive_session(cwd: Option<&str>, args: &str) -> bool {
    cwd.is_some_and(|c| c != "/")
        && !args
            .split_whitespace()
            .any(|word| matches!(word, "app-server" | "mcp-server"))
}

fn process_args(pid: i32) -> String {
    Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// The pid and cwd of each interactive `codex` that holds no top-level rollout
/// open yet: a session at its first prompt, or blocked on a question before its
/// first turn. Only these cost the extra `ps` call.
pub fn rolloutless_sessions(processes: &[ProcessOpenRollouts]) -> Vec<(i32, String)> {
    processes
        .iter()
        .filter(|p| p.open_session_ids.is_empty())
        .filter(|p| is_interactive_session(p.cwd.as_deref(), &process_args(p.pid)))
        .filter_map(|p| Some((p.pid, p.cwd.clone()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: i32, open: &[&str]) -> ProcessOpenRollouts {
        ProcessOpenRollouts {
            pid,
            cwd: Some("/code/app".to_string()),
            open_session_ids: open.iter().map(ToString::to_string).collect(),
        }
    }

    // Two codex processes in one folder each own their session (a verifier case).
    #[test]
    fn open_file_evidence_ties_each_process_to_its_own_rollout() {
        let live = resolve_liveness_by_open_files(&[
            process(1, &["own-session"]),
            process(2, &["another-session"]),
        ]);
        assert_eq!(live.get("own-session"), Some(&1));
        assert_eq!(live.get("another-session"), Some(&2));
        assert_eq!(live.len(), 2);
    }

    // A `codex` process holds no rollout open for the ~10s between starting and writing its
    // first turn (rollouts are created lazily). Without this, a naive cwd-based fallback would
    // misattribute the cwd's previous, now-finished, rollout as still live.
    #[test]
    fn a_new_process_with_no_rollout_yet_is_not_live() {
        assert!(resolve_liveness_by_open_files(&[process(9, &[])]).is_empty());
    }

    // A process holding more than one top-level rollout open has not happened in the rollouts
    // inspected on this Mac, but ambiguous evidence should not be guessed at either.
    #[test]
    fn a_process_with_more_than_one_open_top_level_rollout_is_ambiguous_and_not_live() {
        assert!(resolve_liveness_by_open_files(&[process(9, &["a", "b"])]).is_empty());
    }

    #[test]
    fn lsof_output_gives_the_cwd_and_the_open_files() {
        let text = "p42\nfcwd\nn/code/app\nftxt\nn/usr/bin/codex\nf12\nn/home/me/.codex/sessions/r.jsonl\n";
        let (cwd, files) = parse_lsof(text);
        assert_eq!(cwd.as_deref(), Some("/code/app"));
        assert_eq!(
            files,
            vec!["/usr/bin/codex", "/home/me/.codex/sessions/r.jsonl"]
        );
    }

    #[test]
    fn only_a_cli_session_in_a_real_folder_is_an_interactive_session() {
        assert!(is_interactive_session(
            Some("/code/app"),
            "/bin/codex -c x=y"
        ));
        assert!(is_interactive_session(
            Some("/code/app"),
            "codex resume abc"
        ));
        // The desktop app's embedded codex runs from `/`.
        assert!(!is_interactive_session(
            Some("/"),
            "/Applications/ChatGPT.app/codex"
        ));
        // IDE extensions and the desktop app run servers from a workspace.
        assert!(!is_interactive_session(
            Some("/code/app"),
            "/ext/codex app-server --listen"
        ));
        assert!(!is_interactive_session(
            Some("/code/app"),
            "codex mcp-server"
        ));
        // A prompt that merely contains the word inside another word is a session.
        assert!(is_interactive_session(
            Some("/code/app"),
            "codex fix my-app-server-thing"
        ));
        // lsof could not read the cwd (the process went away): nothing to show.
        assert!(!is_interactive_session(None, "codex"));
    }

    #[test]
    fn a_process_with_a_rollout_open_never_needs_a_placeholder() {
        assert!(rolloutless_sessions(&[process(1, &["s"]), process(2, &["a", "b"])]).is_empty());
    }
}
