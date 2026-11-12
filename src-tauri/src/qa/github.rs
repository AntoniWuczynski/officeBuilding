//! `gh`: the signed-in user's pull requests, their diffs, and the one call that posts a review.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use super::review::HEADER;
use crate::process::{Call, Captured, Runner, login_paths};

const LIST_LIMIT: Duration = Duration::from_secs(30);
const DIFF_LIMIT: Duration = Duration::from_secs(60);
const POST_LIMIT: Duration = Duration::from_secs(30);

/// The longest `gh` complaint kept.
const WHY_MAX: usize = 200;

/// A repository on github.com.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    owner: String,
    name: String,
}

/// Whether `part` is a safe owner or repository name: 1 to 100 of letters, digits, `-`, `_`
/// and `.`, not starting with `-` or `.`.
fn valid_part(part: &str) -> bool {
    (1..=100).contains(&part.len())
        && !part.starts_with(['-', '.'])
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

impl Repo {
    /// From "owner/name".
    #[must_use]
    pub fn parse(slug: &str) -> Option<Self> {
        let (owner, name) = slug.split_once('/')?;
        (valid_part(owner) && valid_part(name)).then(|| Self {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    /// From a git remote URL on github.com: `https://github.com/o/n(.git)`,
    /// `git@github.com:o/n(.git)` or `ssh://git@github.com/o/n(.git)`. None for anything else.
    #[must_use]
    pub fn from_remote(url: &str) -> Option<Self> {
        let url = url.trim_end_matches('/');
        let url = url.strip_suffix(".git").unwrap_or(url);
        let path = [
            "https://github.com/",
            "git@github.com:",
            "ssh://git@github.com/",
        ]
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
        Self::parse(path)
    }
}

impl fmt::Display for Repo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    /// 40 hex characters.
    pub head: String,
    pub draft: bool,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhError {
    /// `gh` is not signed in (exit code 4).
    SignedOut,
    /// GitHub asked the app to slow down.
    RateLimited,
    /// Anything else: why, in words.
    Failed(String),
}

impl GhError {
    #[must_use]
    pub fn words(&self) -> String {
        match self {
            Self::SignedOut => {
                "gh is not signed in to GitHub. Run gh auth login in a terminal".into()
            }
            Self::RateLimited => "GitHub asked the app to slow down".into(),
            Self::Failed(why) => format!("GitHub could not be asked: {why}"),
        }
    }
}

/// Where the login shell finds the two commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub gh: Option<PathBuf>,
    pub git: Option<PathBuf>,
}

/// One login-shell run: `command -v gh; command -v git`.
#[must_use]
pub fn find(runner: &dyn Runner, shell: &str) -> Found {
    let paths = login_paths(runner, shell, ["gh", "git"]);
    let named = |name: &str| {
        paths
            .iter()
            .find(|p| p.file_name().is_some_and(|f| f == name))
            .cloned()
    };
    Found {
        gh: named("gh"),
        git: named("git"),
    }
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    number: u64,
    title: String,
    head_ref_oid: String,
    is_draft: bool,
    #[serde(default)]
    labels: Vec<Label>,
}

#[derive(Deserialize)]
struct Posted {
    id: u64,
}

pub struct GitHub {
    runner: Arc<dyn Runner>,
    gh: PathBuf,
}

impl GitHub {
    #[must_use]
    pub fn new(runner: Arc<dyn Runner>, gh: PathBuf) -> Self {
        Self { runner, gh }
    }

    /// Run `args` and return stdout when `gh` exits 0.
    fn run(&self, args: &[String], limit: Duration) -> Result<String, GhError> {
        let call = Call::new(self.gh.clone(), args);
        let done = self
            .runner
            .capture_within(&call, limit)
            .map_err(GhError::Failed)?;
        failure(&done).map_or(Ok(done.stdout), Err)
    }

    /// The signed-in user's open pull requests on `repo`.
    pub fn mine(&self, repo: &Repo) -> Result<Vec<PullRequest>, GhError> {
        let args = strings(&[
            "pr",
            "list",
            "-R",
            &repo.to_string(),
            "--author",
            "@me",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title,headRefOid,isDraft,labels",
        ]);
        let out = self.run(&args, LIST_LIMIT)?;
        let listed: Vec<Listed> = serde_json::from_str(&out).map_err(|_| unreadable())?;
        Ok(listed
            .into_iter()
            .filter(|l| {
                l.head_ref_oid.len() == 40 && l.head_ref_oid.chars().all(|c| c.is_ascii_hexdigit())
            })
            .map(|l| PullRequest {
                number: l.number,
                title: l.title,
                head: l.head_ref_oid,
                draft: l.is_draft,
                labels: l.labels.into_iter().map(|label| label.name).collect(),
            })
            .collect())
    }

    /// The pull request's diff against its base.
    pub fn diff(&self, repo: &Repo, number: u64) -> Result<String, GhError> {
        let args = strings(&[
            "pr",
            "diff",
            &number.to_string(),
            "-R",
            &repo.to_string(),
            "--color",
            "never",
        ]);
        self.run(&args, DIFF_LIMIT)
    }

    /// Post the review in `payload` (a JSON file). Returns the new review's id.
    pub fn post(&self, repo: &Repo, number: u64, payload: &Path) -> Result<u64, GhError> {
        let mut args = strings(&[
            "api",
            &format!("repos/{repo}/pulls/{number}/reviews"),
            "--method",
            "POST",
            "--input",
        ]);
        args.push(payload.to_string_lossy().into_owned());
        let out = self.run(&args, POST_LIMIT)?;
        let posted: Posted = serde_json::from_str(&out).map_err(|_| unreadable())?;
        Ok(posted.id)
    }

    /// The id of a review this app already posted on `head` of the pull request, if any.
    /// Asked before a post is repeated, because a post that failed on this side may have
    /// reached GitHub.
    pub fn review_of(&self, repo: &Repo, number: u64, head: &str) -> Result<Option<u64>, GhError> {
        let args = strings(&[
            "api",
            &format!("repos/{repo}/pulls/{number}/reviews"),
            "--paginate",
            "--jq",
            r#".[] | [.id, .commit_id, (.body // "")] | @tsv"#,
        ]);
        let out = self.run(&args, LIST_LIMIT)?;
        Ok(out.lines().find_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let (id, commit, body) = (fields.next()?, fields.next()?, fields.next()?);
            (commit == head && body.starts_with(HEADER))
                .then(|| id.parse().ok())
                .flatten()
        }))
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_string()).collect()
}

fn unreadable() -> GhError {
    GhError::Failed("GitHub's answer could not be read".into())
}

/// Why a finished call failed, or None when it exited 0.
fn failure(done: &Captured) -> Option<GhError> {
    match done.code {
        Some(0) => None,
        None => Some(GhError::Failed("GitHub did not answer in time".into())),
        Some(4) => Some(GhError::SignedOut),
        Some(code) => Some(if done.stderr.to_lowercase().contains("rate limit") {
            GhError::RateLimited
        } else {
            GhError::Failed(super::first_line(&done.stderr).map_or_else(
                || format!("gh stopped with exit code {code}"),
                |line| line.chars().take(WHY_MAX).collect(),
            ))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::FakeRunner;

    const LIST: &str = include_str!("fixtures/pr-list.json");
    const POSTED: &str = include_str!("fixtures/review-posted.json");
    const GH: &str = "/opt/homebrew/bin/gh";

    fn repo() -> Repo {
        Repo::parse("example-org/widgets").expect("repo")
    }

    fn answering(code: Option<i32>, stdout: &str, stderr: &str) -> Arc<FakeRunner> {
        let done = Captured {
            code,
            stdout: stdout.into(),
            stderr: stderr.into(),
        };
        Arc::new(FakeRunner::new(move |_| Ok(done.clone())))
    }

    fn client(fake: &Arc<FakeRunner>) -> GitHub {
        GitHub::new(fake.clone(), PathBuf::from(GH))
    }

    fn args(call: &Call) -> Vec<String> {
        call.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn mine_asks_with_the_exact_arguments_and_keeps_the_valid_pull_requests() {
        let fake = answering(Some(0), LIST, "");
        let got = client(&fake).mine(&repo()).expect("mine");
        let calls = fake.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].program, Path::new(GH));
        assert_eq!(
            args(&calls[0]),
            strings(&[
                "pr",
                "list",
                "-R",
                "example-org/widgets",
                "--author",
                "@me",
                "--state",
                "open",
                "--limit",
                "100",
                "--json",
                "number,title,headRefOid,isDraft,labels",
            ])
        );
        assert_eq!(fake.limits(), vec![LIST_LIMIT]);
        let numbers: Vec<u64> = got.iter().map(|p| p.number).collect();
        assert_eq!(numbers, vec![12, 13, 14]);
        assert_eq!(got[0].title, "Add a sprocket cache");
        assert_eq!(got[0].head, "0123456789abcdef0123456789abcdef01234567");
        assert!(!got[0].draft && got[1].draft);
        assert_eq!(got[1].labels, vec!["wip"]);
        assert_eq!(got[2].labels, vec!["no-qa"]);
    }

    #[test]
    fn diff_asks_with_the_exact_arguments_and_returns_stdout() {
        let fake = answering(Some(0), "diff --git a/x b/x\n", "");
        let got = client(&fake).diff(&repo(), 12).expect("diff");
        assert_eq!(got, "diff --git a/x b/x\n");
        assert_eq!(
            args(&fake.calls()[0]),
            strings(&[
                "pr",
                "diff",
                "12",
                "-R",
                "example-org/widgets",
                "--color",
                "never"
            ])
        );
        assert_eq!(fake.limits(), vec![DIFF_LIMIT]);
    }

    #[test]
    fn post_asks_with_the_exact_arguments_and_returns_the_review_id() {
        let fake = answering(Some(0), POSTED, "");
        let got = client(&fake)
            .post(&repo(), 12, Path::new("/data/qa/runs/review.json"))
            .expect("post");
        assert_eq!(got, 90210);
        assert_eq!(
            args(&fake.calls()[0]),
            strings(&[
                "api",
                "repos/example-org/widgets/pulls/12/reviews",
                "--method",
                "POST",
                "--input",
                "/data/qa/runs/review.json",
            ])
        );
        assert_eq!(fake.limits(), vec![POST_LIMIT]);
    }

    #[test]
    fn review_of_finds_our_review_on_that_head_only() {
        let other = "b".repeat(40);
        let head = "a".repeat(40);
        let listing = format!(
            "11\t{head}\tSomeone else's review\n12\t{other}\t{HEADER}\\n\\nNo issues found.\n\
             13\t{head}\t{HEADER}\\n\\nNo issues found.\n"
        );
        let fake = answering(Some(0), &listing, "");
        let got = client(&fake).review_of(&repo(), 12, &head).expect("listed");
        assert_eq!(got, Some(13));
        assert_eq!(
            args(&fake.calls()[0])[..3],
            strings(&[
                "api",
                "repos/example-org/widgets/pulls/12/reviews",
                "--paginate"
            ])
        );
        let none = answering(Some(0), "", "");
        assert_eq!(client(&none).review_of(&repo(), 12, &head), Ok(None));
    }

    #[test]
    fn a_finished_call_maps_to_an_error() {
        let long = "x".repeat(300);
        let cases: Vec<(Option<i32>, &str, GhError)> = vec![
            (Some(4), "", GhError::SignedOut),
            (
                Some(1),
                "gh: API rate limit exceeded for user\n",
                GhError::RateLimited,
            ),
            (
                Some(1),
                "\n  Could not resolve to a Repository\nmore\n",
                GhError::Failed("Could not resolve to a Repository".into()),
            ),
            (Some(1), &long, GhError::Failed("x".repeat(WHY_MAX))),
            (
                Some(1),
                "",
                GhError::Failed("gh stopped with exit code 1".into()),
            ),
            (
                None,
                "",
                GhError::Failed("GitHub did not answer in time".into()),
            ),
        ];
        for (code, stderr, want) in cases {
            let fake = answering(code, "", stderr);
            assert_eq!(
                client(&fake).diff(&repo(), 1),
                Err(want.clone()),
                "{want:?}"
            );
        }
    }

    #[test]
    fn a_runner_error_and_unreadable_output_are_failures() {
        let fake = Arc::new(FakeRunner::new(|_| Err("gh could not be started".into())));
        assert_eq!(
            client(&fake).mine(&repo()),
            Err(GhError::Failed("gh could not be started".into()))
        );
        let fake = answering(Some(0), "not json", "");
        let want = Err(GhError::Failed("GitHub's answer could not be read".into()));
        assert_eq!(client(&fake).mine(&repo()), want);
        assert_eq!(
            client(&fake).post(&repo(), 1, Path::new("/p.json")),
            Err(GhError::Failed("GitHub's answer could not be read".into()))
        );
    }

    #[test]
    fn words_say_what_to_do() {
        assert_eq!(
            GhError::SignedOut.words(),
            "gh is not signed in to GitHub. Run gh auth login in a terminal"
        );
        assert_eq!(
            GhError::RateLimited.words(),
            "GitHub asked the app to slow down"
        );
        assert_eq!(
            GhError::Failed("boom".into()).words(),
            "GitHub could not be asked: boom"
        );
    }

    #[test]
    fn from_remote_reads_the_three_forms_with_and_without_git() {
        for url in [
            "https://github.com/example-org/widgets",
            "https://github.com/example-org/widgets.git",
            "https://github.com/example-org/widgets/",
            "git@github.com:example-org/widgets",
            "git@github.com:example-org/widgets.git",
            "ssh://git@github.com/example-org/widgets",
            "ssh://git@github.com/example-org/widgets.git",
        ] {
            assert_eq!(Repo::from_remote(url), Some(repo()), "{url}");
        }
    }

    #[test]
    fn from_remote_refuses_what_is_not_a_github_repo() {
        for url in [
            "https://gitlab.com/example-org/widgets",
            "https://github.com.evil.example/example-org/widgets",
            "https://github.com/example-org",
            "https://github.com/example-org/",
            "https://github.com/-example/widgets",
            "https://github.com/example-org/-widgets",
            "https://github.com/example-org/wid gets",
            "https://github.com/example-org/widgets/extra",
            "",
        ] {
            assert_eq!(Repo::from_remote(url), None, "{url:?}");
        }
    }

    #[test]
    fn parse_and_display_round_trip() {
        assert_eq!(repo().to_string(), "example-org/widgets");
        assert_eq!(Repo::parse(&repo().to_string()), Some(repo()));
        assert_eq!(Repo::parse("widgets"), None);
        assert_eq!(Repo::parse(".hidden/widgets"), None);
        assert_eq!(Repo::parse(&format!("o/{}", "n".repeat(101))), None);
    }

    #[test]
    fn find_reads_both_paths_from_the_login_shell() {
        let fake = answering(
            Some(0),
            "Last login\n/opt/homebrew/bin/gh\n/usr/bin/git\n",
            "",
        );
        let got = find(fake.as_ref(), "/bin/zsh");
        assert_eq!(got.gh, Some(PathBuf::from("/opt/homebrew/bin/gh")));
        assert_eq!(got.git, Some(PathBuf::from("/usr/bin/git")));
        assert_eq!(
            fake.calls(),
            vec![Call::new(
                "/bin/zsh",
                ["-l", "-c", "command -v gh; command -v git"]
            )]
        );
    }

    #[test]
    fn find_gives_none_for_a_missing_command() {
        let fake = answering(Some(1), "/usr/bin/git\n", "");
        assert_eq!(
            find(fake.as_ref(), "/bin/zsh"),
            Found {
                gh: None,
                git: Some(PathBuf::from("/usr/bin/git"))
            }
        );
        let fake = Arc::new(FakeRunner::new(|_| Err("no shell".into())));
        assert_eq!(find(fake.as_ref(), "/bin/zsh"), Found::default());
    }
}
