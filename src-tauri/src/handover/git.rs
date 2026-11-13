//! A read-only summary of a desk's folder for the brief: three git calls that write nothing
//! to the repo and run no program the repo configures (T17).

use super::{head, tidy, tidy_line};
use crate::process::{Call, Captured, Runner};
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

/// How long each git call may run.
pub const CALL_LIMIT: Duration = Duration::from_secs(5);
/// Lines of one call's output kept.
const LINES_MAX: usize = 150;
/// Characters of one call's output kept.
const CHARS_MAX: usize = 8000;
/// Characters of the reason a call is not available.
const REASON_MAX: usize = 200;

const NO_GIT: &str = "git was not found, so the folder's state is not included.";

/// Options that keep git from taking the index lock or asking a file system monitor.
const QUIET: &[&str] = &["--no-optional-locks", "-c", "core.fsmonitor=false"];

/// Each call: the options that go after `-C <folder>`, then the words shown and run.
const CALLS: [(&[&str], &[&str]); 3] = [
    (QUIET, &["status", "--porcelain=v1", "--branch"]),
    (
        QUIET,
        &["diff", "--no-ext-diff", "--no-textconv", "--stat", "HEAD"],
    ),
    // A repo can set `log.showSignature`, which makes `git log` run its `gpg.program`.
    (
        &["--no-optional-locks", "-c", "log.showSignature=false"],
        &["log", "-5", "--oneline", "--no-decorate"],
    ),
];

/// What a call printed, cut to its first lines and characters.
fn output(stdout: &str) -> String {
    let text = tidy(stdout);
    if text.is_empty() {
        return "(no output)".to_owned();
    }
    let lines: Vec<&str> = text.lines().collect();
    let joined = lines
        .iter()
        .take(LINES_MAX)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    let (kept, more) = head(&joined, CHARS_MAX);
    if lines.len() > LINES_MAX || more > 0 {
        format!("{}\n… cut", kept.trim_end())
    } else {
        joined
    }
}

fn not_available(reason: &str) -> String {
    format!(
        "(not available: {})",
        head(&tidy_line(reason), REASON_MAX).0
    )
}

fn block(got: Result<Captured, String>) -> String {
    match got {
        Ok(Captured {
            code: Some(0),
            stdout,
            ..
        }) => output(&stdout),
        Ok(Captured {
            code: Some(code),
            stderr,
            ..
        }) => {
            let first = tidy(&stderr).lines().next().unwrap_or_default().to_owned();
            if first.is_empty() {
                not_available(&format!("git exited with code {code}"))
            } else {
                not_available(&first)
            }
        }
        Ok(Captured { code: None, .. }) => not_available("git did not finish in time"),
        Err(reason) => not_available(&reason),
    }
}

/// A read-only summary of `folder`'s git state as text. Never fails: what could not be read
/// is said in words.
#[must_use]
pub fn summary(runner: &dyn Runner, git: Option<&Path>, folder: &Path) -> String {
    let Some(git) = git else {
        return NO_GIT.to_owned();
    };
    CALLS
        .iter()
        .map(|(options, words)| {
            let mut args: Vec<OsString> = vec!["-C".into(), folder.as_os_str().to_owned()];
            args.extend(options.iter().chain(words.iter()).map(OsString::from));
            let got = runner.capture_within(&Call::new(git, args), CALL_LIMIT);
            format!("$ git {}\n{}", words.join(" "), block(got))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::FakeRunner;

    const GIT: &str = "/usr/bin/git";

    fn printed(stdout: &str) -> Captured {
        Captured {
            code: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    fn args(call: &Call) -> Vec<String> {
        call.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_three_calls_are_argument_vectors_with_the_folder_on_its_own() {
        for name in ["--help", "a b 'c\" $(x)"] {
            let fake = FakeRunner::new(|_| Ok(printed("x")));
            let folder = Path::new("/work").join(name);
            let _ = summary(&fake, Some(Path::new(GIT)), &folder);
            let f = folder.to_string_lossy().into_owned();
            let want: Vec<Vec<String>> = [
                vec![
                    "-C",
                    &f,
                    "--no-optional-locks",
                    "-c",
                    "core.fsmonitor=false",
                    "status",
                    "--porcelain=v1",
                    "--branch",
                ],
                vec![
                    "-C",
                    &f,
                    "--no-optional-locks",
                    "-c",
                    "core.fsmonitor=false",
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--stat",
                    "HEAD",
                ],
                vec![
                    "-C",
                    &f,
                    "--no-optional-locks",
                    "-c",
                    "log.showSignature=false",
                    "log",
                    "-5",
                    "--oneline",
                    "--no-decorate",
                ],
            ]
            .into_iter()
            .map(|v| v.into_iter().map(str::to_owned).collect())
            .collect();
            let calls = fake.calls();
            assert_eq!(calls.iter().map(args).collect::<Vec<_>>(), want);
            assert!(calls.iter().all(|c| c.program == Path::new(GIT)));
            assert_eq!(fake.limits(), vec![Duration::from_secs(5); 3]);
        }
    }

    #[test]
    fn the_blocks_name_the_words_never_the_folder() {
        let fake = FakeRunner::new(|call| {
            if args(call).contains(&"status".to_owned()) {
                Ok(printed("## main\n M src/a.rs\n"))
            } else if args(call).contains(&"diff".to_owned()) {
                Ok(printed(""))
            } else {
                Ok(printed("abc123 First commit\n"))
            }
        });
        let got = summary(&fake, Some(Path::new(GIT)), Path::new("/secret/folder"));
        assert_eq!(
            got,
            "$ git status --porcelain=v1 --branch\n## main\n M src/a.rs\n\n\
             $ git diff --no-ext-diff --no-textconv --stat HEAD\n(no output)\n\n\
             $ git log -5 --oneline --no-decorate\nabc123 First commit"
        );
    }

    #[test]
    fn a_call_that_fails_says_why_and_the_others_still_appear() {
        let fake = FakeRunner::new(|call| {
            let a = args(call);
            if a.contains(&"status".to_owned()) {
                Ok(Captured {
                    code: Some(128),
                    stdout: String::new(),
                    stderr: "fatal: not a git repository\nmore\n".into(),
                })
            } else if a.contains(&"diff".to_owned()) {
                Ok(Captured::default())
            } else {
                Err("/usr/bin/git could not be started: denied".into())
            }
        });
        let got = summary(&fake, Some(Path::new(GIT)), Path::new("/w"));
        assert_eq!(
            got,
            "$ git status --porcelain=v1 --branch\n(not available: fatal: not a git repository)\n\n\
             $ git diff --no-ext-diff --no-textconv --stat HEAD\n\
             (not available: git did not finish in time)\n\n\
             $ git log -5 --oneline --no-decorate\n\
             (not available: /usr/bin/git could not be started: denied)"
        );
    }

    #[test]
    fn a_failure_with_nothing_on_standard_error_gives_its_code_and_a_long_reason_is_cut() {
        let long = "e".repeat(500);
        let fake = FakeRunner::new(move |call| {
            if args(call).contains(&"status".to_owned()) {
                Ok(Captured {
                    code: Some(1),
                    ..Captured::default()
                })
            } else {
                Err(long.clone())
            }
        });
        let got = summary(&fake, Some(Path::new(GIT)), Path::new("/w"));
        assert!(
            got.contains("(not available: git exited with code 1)"),
            "{got}"
        );
        assert!(got.contains(&format!("(not available: {})", "e".repeat(REASON_MAX))));
        assert!(!got.contains(&"e".repeat(REASON_MAX + 1)));
    }

    #[test]
    fn no_git_gives_the_one_sentence() {
        let fake = FakeRunner::new(|_| Ok(printed("x")));
        assert_eq!(summary(&fake, None, Path::new("/w")), NO_GIT);
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn long_output_is_cut() {
        let many = (0..200)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let fake = FakeRunner::new(move |_| Ok(printed(&many)));
        let got = summary(&fake, Some(Path::new(GIT)), Path::new("/w"));
        let status = got.split("\n\n").next().expect("first block");
        assert!(status.contains("line 149\n… cut"), "{status}");
        assert!(!status.contains("line 150"));
        assert_eq!(status.lines().count(), 1 + LINES_MAX + 1);

        let wide = "w".repeat(CHARS_MAX + 100);
        let fake = FakeRunner::new(move |_| Ok(printed(&wide)));
        let got = summary(&fake, Some(Path::new(GIT)), Path::new("/w"));
        let status = got.split("\n\n").next().expect("first block");
        assert!(status.ends_with(&format!("{}\n… cut", "w".repeat(CHARS_MAX))));
    }

    #[test]
    fn escape_sequences_in_the_output_are_gone() {
        let fake = FakeRunner::new(|_| Ok(printed("\x1b[31m M a.rs\x1b[0m\x07\n")));
        let got = summary(&fake, Some(Path::new(GIT)), Path::new("/w"));
        assert!(!got.contains('\x1b'), "{got:?}");
        assert!(!got.contains('\x07'));
        assert!(
            got.starts_with("$ git status --porcelain=v1 --branch\nM a.rs\n\n"),
            "{got:?}"
        );
    }
}
