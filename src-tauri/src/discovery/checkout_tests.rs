//! `locate`, `branch_of` and `assemble` against a real repository with a linked worktree.
//! These tests run `git`, as the app itself never does.

use super::claude_code::{Found, LiveEntry, parse_transcript};
use super::*;
use std::io::Cursor;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("these tests need git on the PATH");
    assert!(status.success(), "git {args:?} failed");
}

struct Repo {
    base: PathBuf,
    main: PathBuf,
    tree: PathBuf,
}

impl Repo {
    /// A repository `main repo` on `main` with a linked worktree `side tree` on
    /// `feature/side`. The temp folder is canonicalised because git writes resolved paths
    /// into the worktree's `gitdir:` line and macOS's temp folder is a symlink.
    fn new(tag: &str) -> Self {
        let base = fs::canonicalize(std::env::temp_dir())
            .expect("temp dir")
            .join(format!("ob-checkout-{}-{tag}", std::process::id()));
        let main = base.join("main repo");
        let tree = base.join("side tree");
        fs::create_dir_all(main.join("web")).expect("mkdir");
        git(&main, &["init", "-q", "-b", "main"]);
        git(
            &main,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "init",
            ],
        );
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature/side",
                &tree.to_string_lossy(),
            ],
        );
        fs::create_dir_all(tree.join("server")).expect("mkdir");
        Self { base, main, tree }
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn line(cwd: &Path) -> String {
    let cwd = cwd.display();
    format!(
        r#"{{"type":"assistant","cwd":"{cwd}","timestamp":"2026-09-29T10:00:00Z","message":{{"id":"m","content":[{{"type":"text","text":"ok"}}]}}}}"#
    )
}

/// A session that started in `started` and is now in `now`.
fn input(id: &str, started: &Path, now: &Path, live: bool) -> SessionInput {
    let f = Found {
        session_id: id.to_string(),
        transcript: parse_transcript(Cursor::new(format!("{}\n{}", line(started), line(now)))),
        live: live.then(|| LiveEntry {
            pid: 1,
            session_id: id.to_string(),
            cwd: started.to_string_lossy().into_owned(),
            busy: false,
            waiting: false,
            name: None,
        }),
        helpers: Vec::new(),
        helper_places: BTreeMap::new(),
    };
    from_claude(f, &[]).expect("input")
}

fn run(inputs: Vec<SessionInput>, before: &BTreeMap<String, PathBuf>) -> Discovered {
    assemble(Path::new("/nowhere"), inputs, locate, before)
}

#[test]
fn locate_finds_the_top_of_each_checkout() {
    let repo = Repo::new("locate");
    let in_main = locate(&repo.main.join("web"));
    assert_eq!(in_main.root, repo.main);
    assert_eq!(in_main.checkout, repo.main);
    assert!(!in_main.worktree);
    let in_tree = locate(&repo.tree.join("server"));
    assert_eq!(in_tree.root, repo.main);
    assert_eq!(in_tree.checkout, repo.tree);
    assert!(in_tree.worktree);
}

#[test]
fn each_session_opens_the_checkout_it_works_in() {
    let repo = Repo::new("assemble");
    let outside = repo.base.join("scratch");
    fs::create_dir_all(&outside).expect("mkdir");
    let main_web = repo.main.join("web");
    let tree_server = repo.tree.join("server");
    let d = run(
        vec![
            input("a", &main_web, &main_web, true),
            input("b", &tree_server, &tree_server, true),
            input("c", &main_web, &tree_server, true),
            input("d", &main_web, &outside, true),
            input("e", &tree_server, &tree_server, false),
        ],
        &BTreeMap::new(),
    );
    let checkout = |id: &str| d.checkouts.get(id).cloned();
    assert_eq!(checkout("a"), Some(repo.main.clone()));
    assert_eq!(checkout("b"), Some(repo.tree.clone()));
    assert_eq!(checkout("c"), Some(repo.tree.clone()));
    assert_eq!(checkout("d"), Some(repo.main.clone()));
    assert_eq!(checkout("e"), Some(repo.tree.clone()));
    let seen = |id: &str| {
        let s = d.sessions.iter().find(|s| s.id == id).expect("session");
        (s.worktree, s.branch.clone())
    };
    assert_eq!(seen("a"), (false, None));
    assert_eq!(seen("b"), (true, Some("feature/side".to_string())));
    assert_eq!(seen("c"), (true, Some("feature/side".to_string())));
    assert_eq!(seen("d"), (false, None));
}

#[test]
fn helpers_get_a_folder_only_inside_their_sessions_repo() {
    let repo = Repo::new("helpers");
    let unrelated = repo.base.join("elsewhere");
    fs::create_dir_all(unrelated.join(".git")).expect("mkdir");
    let main_web = repo.main.join("web");
    let mut one = input("s", &main_web, &main_web, true);
    one.helper_places = [
        ("in-tree", repo.tree.join("server")),
        ("in-main", main_web.clone()),
        ("away", unrelated),
    ]
    .into_iter()
    .map(|(id, dir)| (id.to_string(), locate(&dir)))
    .collect();
    let d = run(vec![one], &BTreeMap::new());
    assert_eq!(
        d.checkouts.get(&helper_key("s", "in-tree")),
        Some(&repo.tree)
    );
    assert_eq!(
        d.checkouts.get(&helper_key("s", "in-main")),
        Some(&repo.main)
    );
    assert_eq!(d.checkouts.get(&helper_key("s", "away")), None);
}

#[test]
fn branch_of_reads_head_and_ignores_what_is_not_a_branch() {
    let repo = Repo::new("branch");
    assert_eq!(branch_of(&repo.main), Some("main".to_string()));
    assert_eq!(branch_of(&repo.tree), Some("feature/side".to_string()));
    assert_eq!(branch_of(&repo.base), None);
    fs::write(repo.main.join(".git/HEAD"), "ref: refs/heads/.invalid\n").expect("write");
    assert_eq!(branch_of(&repo.main), None);
    git(&repo.tree, &["checkout", "-q", "--detach"]);
    assert_eq!(branch_of(&repo.tree), None);
}

#[test]
fn a_removed_worktree_is_remembered_while_the_session_stays_inside_it() {
    let repo = Repo::new("lost");
    let main_web = repo.main.join("web");
    let tree_server = repo.tree.join("server");
    let inputs = || {
        vec![
            input("moved", &main_web, &tree_server, true),
            input("left", &main_web, &tree_server, true),
        ]
    };
    let first = run(inputs(), &BTreeMap::new());
    assert_eq!(first.checkouts.get("moved"), Some(&repo.tree));
    fs::remove_dir_all(&repo.tree).expect("remove");
    let mut again = inputs();
    // `left` has since gone back to the main checkout.
    again[1] = input("left", &main_web, &main_web, true);
    let second = run(again, &first.checkouts);
    assert_eq!(second.checkouts.get("moved"), Some(&repo.tree));
    assert_eq!(second.checkouts.get("left"), Some(&repo.main));
}
