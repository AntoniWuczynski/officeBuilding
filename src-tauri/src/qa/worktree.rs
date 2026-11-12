//! `git`: a pull request's head in a throwaway worktree.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::process::{Call, Captured, Runner};

const QUICK: Duration = Duration::from_secs(5);
const FETCH_LIMIT: Duration = Duration::from_secs(120);
const ADD_LIMIT: Duration = Duration::from_secs(120);
const REMOVE_LIMIT: Duration = Duration::from_secs(30);

pub struct Git {
    runner: Arc<dyn Runner>,
    git: PathBuf,
}

/// Git options for a checkout that runs nothing: no hooks (a `post-checkout` hook can run code
/// the pull request changed) and no Large File Storage downloads.
const INERT: [&str; 8] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "filter.lfs.smudge=",
    "-c",
    "filter.lfs.process=",
    "-c",
    "filter.lfs.required=false",
];

impl Git {
    #[must_use]
    pub fn new(runner: Arc<dyn Runner>, git: PathBuf) -> Self {
        Self { runner, git }
    }

    /// Run `git -C <root> <args>`.
    fn run(&self, root: &Path, args: &[&str], limit: Duration) -> Result<Captured, String> {
        let mut call = Call::new(self.git.clone(), ["-C"]);
        call.args.push(root.as_os_str().to_owned());
        call.args.extend(args.iter().map(Into::into));
        self.runner.capture_within(&call, limit)
    }

    /// Like `run`, but a call that did not exit 0 is an error starting with `what`.
    fn run_ok(
        &self,
        root: &Path,
        args: &[&str],
        limit: Duration,
        what: &str,
    ) -> Result<(), String> {
        let done = self.run(root, args, limit)?;
        match done.code {
            Some(0) => Ok(()),
            None => Err("git did not finish in time".into()),
            Some(_) => Err(format!(
                "{what}: {}",
                super::first_line(&done.stderr).unwrap_or("git gave no reason")
            )),
        }
    }

    /// The URL of the repo's `origin`, or None when it has none.
    pub fn origin(&self, root: &Path) -> Result<Option<String>, String> {
        let done = self.run(root, &["remote", "get-url", "origin"], QUICK)?;
        Ok((done.code == Some(0)).then(|| done.stdout.trim().to_string()))
    }

    /// Bring a pull request's head into the repo. Creates no branch.
    pub fn fetch_pull(&self, root: &Path, number: u64) -> Result<(), String> {
        let head = format!("pull/{number}/head");
        self.run_ok(
            root,
            &["fetch", "--quiet", "origin", &head],
            FETCH_LIMIT,
            "git could not fetch the pull request",
        )
    }

    /// Whether the repo holds `commit`.
    #[must_use]
    pub fn has_commit(&self, root: &Path, commit: &str) -> bool {
        let object = format!("{commit}^{{commit}}");
        self.run(root, &["cat-file", "-e", &object], QUICK)
            .is_ok_and(|done| done.code == Some(0))
    }

    /// Check `commit` out, detached, in a fresh worktree at `folder`, replacing one left there.
    pub fn add(&self, root: &Path, folder: &Path, commit: &str) -> Result<(), String> {
        self.remove(root, folder)?;
        if let Some(parent) = folder.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("the review folder could not be made: {e}"))?;
        }
        let folder = folder.to_string_lossy();
        let mut args = INERT.to_vec();
        args.extend(["worktree", "add", "--detach", &folder, commit]);
        self.run_ok(
            root,
            &args,
            ADD_LIMIT,
            "git could not check the pull request out",
        )
    }

    /// Remove the worktree at `folder` and forget it. Not being there is not an error.
    pub fn remove(&self, root: &Path, folder: &Path) -> Result<(), String> {
        if folder.exists() {
            let name = folder.to_string_lossy();
            // Best effort: the folder is deleted below whatever git said.
            let _ = self.run(
                root,
                &["worktree", "remove", "--force", &name],
                REMOVE_LIMIT,
            );
            if folder.exists() {
                fs::remove_dir_all(folder)
                    .map_err(|e| format!("the review folder could not be removed: {e}"))?;
            }
        }
        self.run(root, &["worktree", "prune"], QUICK).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::FakeRunner;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const GIT: &str = "/usr/bin/git";
    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    /// A temp folder that goes away with the test.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ob-worktree-{}-{n}", std::process::id()));
            fs::create_dir_all(&dir).expect("temp folder");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn done(code: Option<i32>, stdout: &str, stderr: &str) -> Captured {
        Captured {
            code,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    fn git(fake: &Arc<FakeRunner>) -> Git {
        Git::new(fake.clone(), PathBuf::from(GIT))
    }

    fn answering(code: Option<i32>, stdout: &str, stderr: &str) -> Arc<FakeRunner> {
        let (stdout, stderr) = (stdout.to_string(), stderr.to_string());
        Arc::new(FakeRunner::new(move |_| Ok(done(code, &stdout, &stderr))))
    }

    /// The arguments of each recorded call, as text.
    fn all_args(fake: &FakeRunner) -> Vec<Vec<String>> {
        fake.calls()
            .iter()
            .map(|c| {
                assert_eq!(c.program, Path::new(GIT));
                c.args
                    .iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect()
            })
            .collect()
    }

    fn line(args: &[&str]) -> Vec<String> {
        ["-C", "/repo"]
            .iter()
            .chain(args)
            .map(|a| (*a).to_string())
            .collect()
    }

    #[test]
    fn origin_asks_for_the_remote_url() {
        let fake = answering(Some(0), "git@github.com:example-org/widgets.git\n", "");
        assert_eq!(
            git(&fake).origin(Path::new("/repo")),
            Ok(Some("git@github.com:example-org/widgets.git".into()))
        );
        assert_eq!(
            all_args(&fake),
            vec![line(&["remote", "get-url", "origin"])]
        );
        assert_eq!(fake.limits(), vec![QUICK]);
    }

    #[test]
    fn origin_is_none_on_a_failed_exit_and_an_error_when_git_cannot_start() {
        let fake = answering(Some(2), "", "error: No such remote 'origin'");
        assert_eq!(git(&fake).origin(Path::new("/repo")), Ok(None));
        let fake = Arc::new(FakeRunner::new(|_| Err("git could not be started".into())));
        assert_eq!(
            git(&fake).origin(Path::new("/repo")),
            Err("git could not be started".into())
        );
    }

    #[test]
    fn fetch_pull_fetches_the_head_with_no_branch() {
        let fake = answering(Some(0), "", "");
        assert_eq!(git(&fake).fetch_pull(Path::new("/repo"), 12), Ok(()));
        assert_eq!(
            all_args(&fake),
            vec![line(&["fetch", "--quiet", "origin", "pull/12/head"])]
        );
        assert_eq!(fake.limits(), vec![FETCH_LIMIT]);
    }

    #[test]
    fn fetch_pull_failures_have_their_sentences() {
        let fake = answering(Some(128), "", "\nfatal: couldn't find remote ref\nmore");
        assert_eq!(
            git(&fake).fetch_pull(Path::new("/repo"), 12),
            Err("git could not fetch the pull request: fatal: couldn't find remote ref".into())
        );
        let fake = answering(None, "", "");
        assert_eq!(
            git(&fake).fetch_pull(Path::new("/repo"), 12),
            Err("git did not finish in time".into())
        );
    }

    #[test]
    fn has_commit_follows_the_exit_code() {
        let fake = answering(Some(0), "", "");
        assert!(git(&fake).has_commit(Path::new("/repo"), COMMIT));
        assert_eq!(
            all_args(&fake),
            vec![line(&["cat-file", "-e", &format!("{COMMIT}^{{commit}}")])]
        );
        assert_eq!(fake.limits(), vec![QUICK]);
        for fake in [answering(Some(1), "", ""), answering(None, "", "")] {
            assert!(!git(&fake).has_commit(Path::new("/repo"), COMMIT));
        }
        let fake = Arc::new(FakeRunner::new(|_| Err("gone".into())));
        assert!(!git(&fake).has_commit(Path::new("/repo"), COMMIT));
    }

    #[test]
    fn add_checks_out_detached_after_a_remove_that_skips_a_missing_folder() {
        let tmp = Tmp::new();
        let folder = tmp.path().join("qa").join("worktrees").join("floor");
        let fake = answering(Some(0), "", "");
        assert_eq!(git(&fake).add(Path::new("/repo"), &folder, COMMIT), Ok(()));
        assert_eq!(
            all_args(&fake),
            vec![
                line(&["worktree", "prune"]),
                line(
                    &[
                        &INERT[..],
                        &[
                            "worktree",
                            "add",
                            "--detach",
                            &folder.to_string_lossy(),
                            COMMIT
                        ]
                    ]
                    .concat()
                ),
            ]
        );
        assert_eq!(fake.limits(), vec![QUICK, ADD_LIMIT]);
        assert!(folder.parent().expect("parent").is_dir());
    }

    #[test]
    fn add_removes_a_worktree_left_in_the_folder_first() {
        let tmp = Tmp::new();
        let folder = tmp.path().join("floor");
        fs::create_dir(&folder).expect("folder");
        let fake = answering(Some(0), "", "");
        assert_eq!(git(&fake).add(Path::new("/repo"), &folder, COMMIT), Ok(()));
        let name = folder.to_string_lossy().into_owned();
        let calls = all_args(&fake);
        assert_eq!(calls[0], line(&["worktree", "remove", "--force", &name]));
        assert_eq!(calls[1], line(&["worktree", "prune"]));
        assert_eq!(calls[2][10..13], ["worktree", "add", "--detach"]);
        assert_eq!(calls.len(), 3);
        assert_eq!(fake.limits(), vec![REMOVE_LIMIT, QUICK, ADD_LIMIT]);
    }

    #[test]
    fn add_failures_have_their_sentences() {
        let tmp = Tmp::new();
        let folder = tmp.path().join("floor");
        let fake = Arc::new(FakeRunner::new(|call| {
            if call.args.iter().any(|a| a == "add") {
                Ok(done(Some(128), "", "fatal: invalid reference"))
            } else {
                Ok(done(Some(0), "", ""))
            }
        }));
        assert_eq!(
            git(&fake).add(Path::new("/repo"), &folder, COMMIT),
            Err("git could not check the pull request out: fatal: invalid reference".into())
        );
        let fake = Arc::new(FakeRunner::new(|call| {
            if call.args.iter().any(|a| a == "add") {
                Ok(done(None, "", ""))
            } else {
                Ok(done(Some(0), "", ""))
            }
        }));
        assert_eq!(
            git(&fake).add(Path::new("/repo"), &folder, COMMIT),
            Err("git did not finish in time".into())
        );
    }

    #[test]
    fn remove_on_a_missing_folder_only_prunes() {
        let tmp = Tmp::new();
        let fake = answering(Some(0), "", "");
        let missing = tmp.path().join("nothing");
        assert_eq!(git(&fake).remove(Path::new("/repo"), &missing), Ok(()));
        assert_eq!(all_args(&fake), vec![line(&["worktree", "prune"])]);
    }

    #[test]
    fn remove_deletes_a_folder_that_survives_worktree_remove() {
        let tmp = Tmp::new();
        let folder = tmp.path().join("floor");
        fs::create_dir_all(folder.join("src")).expect("folder");
        fs::write(folder.join("src").join("a.txt"), "x").expect("file");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let fake = {
            let seen = seen.clone();
            let folder = folder.clone();
            Arc::new(FakeRunner::new(move |_| {
                // `worktree remove` leaves the folder in place; prune sees it gone.
                crate::sync::lock(&seen).push(folder.exists());
                Ok(done(Some(1), "", "fatal: cannot remove"))
            }))
        };
        assert_eq!(git(&fake).remove(Path::new("/repo"), &folder), Ok(()));
        assert!(!folder.exists());
        assert_eq!(*crate::sync::lock(&seen), vec![true, false]);
        assert_eq!(fake.limits(), vec![REMOVE_LIMIT, QUICK]);
    }
}
