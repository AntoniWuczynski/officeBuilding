// Blocks commits and pushes that add secrets or private details. The git hooks
// in .githooks/ call it; `pnpm check:sensitive` sweeps the whole history.
//   --staged  what `git commit` is about to record (pre-commit)
//   --push    every commit `git push` is about to send, read from stdin (pre-push)
//   --all     every commit in the repository
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { addedLines, parseDenylist, scan } from "./sensitive.ts";

const NO_COMMIT = /^0+$/;

function git(...args: string[]): string {
  return execFileSync("git", args, { encoding: "utf8", maxBuffer: 1 << 30 });
}

function logOf(...range: string[]): string {
  return git("log", "-p", "-U0", "--no-color", "--no-ext-diff", "--format=commit %H", ...range);
}

/** pre-push gets `<local ref> <local sha> <remote ref> <remote sha>` per ref on stdin. */
function pushedCommits(stdin: string): string {
  return stdin
    .split("\n")
    .filter((l) => l.trim() !== "")
    .map((l) => {
      const [, local, , remote] = l.split(" ");
      if (local === undefined || NO_COMMIT.test(local)) return ""; // deleting a ref sends nothing
      return remote === undefined || NO_COMMIT.test(remote) ? logOf(local, "--not", "--remotes") : logOf(`${remote}..${local}`);
    })
    .join("\n");
}

const mode = process.argv[2];
const diff =
  mode === "--staged"
    ? git("diff", "--cached", "-U0", "--no-color", "--no-ext-diff", "--diff-filter=ACMR")
    : mode === "--push"
      ? pushedCommits(readFileSync(0, "utf8"))
      : mode === "--all"
        ? logOf("--all")
        : null;
if (diff === null) {
  console.error("usage: node scripts/check-sensitive.ts --staged | --push | --all");
  process.exit(2);
}

const denylistPath = join(git("rev-parse", "--show-toplevel").trim(), ".githooks", "sensitive.local.txt");
const denylist = existsSync(denylistPath) ? parseDenylist(readFileSync(denylistPath, "utf8")) : [];
if (denylist.length === 0) {
  console.error(`No private terms in ${denylistPath}, so only generic secrets are checked.`);
}

const findings = scan(addedLines(diff), denylist);
if (findings.length === 0) process.exit(0);

const what = mode === "--staged" ? "this commit" : mode === "--push" ? "the commits being pushed" : "the history";
console.error(`Sensitive information in ${what}:`);
for (const f of findings) {
  const where = f.line === 0 ? f.file : `${f.file}:${f.line}`;
  console.error(`  ${f.commit === null ? "" : `${f.commit.slice(0, 8)} `}${where}  ${f.rule}  ${f.masked}`);
}
console.error(
  `Remove it, or for a false positive add "sensitive-ok" to the line. Private terms come from ${denylistPath} (not committed).`,
);
process.exit(1);
