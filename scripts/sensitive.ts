// The sensitive-information scanner behind the git hooks in .githooks/.
// Generic secret shapes live here. Private words (names of unpublished
// projects, the owner's username) live in a gitignored local denylist, since
// committing them would publish them.

/** One added line of a diff. */
export interface AddedLine {
  readonly file: string;
  readonly line: number;
  readonly text: string;
  /** The commit that added it, when scanning history. */
  readonly commit: string | null;
}

export interface Finding {
  readonly file: string;
  readonly line: number;
  readonly commit: string | null;
  readonly rule: string;
  /** What matched, masked so the report does not repeat the secret. */
  readonly masked: string;
}

interface Rule {
  readonly id: string;
  readonly re: RegExp;
}

/** Secret shapes. Each needs enough characters after its prefix that a placeholder like `sk-ant-...` passes. */
export const SECRET_RULES: readonly Rule[] = [
  { id: "anthropic-key", re: /\bsk-ant-[A-Za-z0-9_-]{20,}/ },
  { id: "openai-key", re: /\bsk-(?:proj-|svcacct-)?[A-Za-z0-9]{8,}[A-Za-z0-9_-]{24,}/ },
  { id: "github-token", re: /\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{50,})/ },
  { id: "aws-access-key", re: /\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/ },
  { id: "slack-token", re: /\bxox[abposr]-[A-Za-z0-9-]{10,}/ },
  { id: "google-api-key", re: /\bAIza[0-9A-Za-z_-]{35}/ },
  { id: "stripe-key", re: /\b[rs]k_live_[0-9A-Za-z]{20,}/ },
  { id: "private-key", re: /-----BEGIN (?:[A-Z]+ )?PRIVATE KEY-----/ },
  { id: "jwt", re: /\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}/ },
  { id: "credential-assignment", re: /\b(?:password|passwd|secret|api[_-]?key|access[_-]?token|auth[_-]?token)["']?\s*[:=]\s*["'][^"'\s]{12,}["']/i },
  // A real macOS home folder names its owner. Fictional ones in tests and docs are fine.
  { id: "home-path", re: /\/Users\/(?!(?:me|you|example|runner|Shared)\/)[A-Za-z0-9._-]+\// },
];

/** Files that should never be committed, whatever they hold. */
const SECRET_FILES: readonly Rule[] = [
  { id: "env-file", re: /(?:^|\/)\.env(?!\.example$)(?:\.[^/]+)?$/ },
  { id: "key-file", re: /\.(?:pem|p12|pfx|key|keystore|jks)$/ },
  { id: "ssh-key-file", re: /(?:^|\/)id_(?:rsa|dsa|ecdsa|ed25519)$/ },
];

/** A line carrying this marker is skipped: for a reviewed false positive. */
export const ALLOW_MARKER = "sensitive-ok";

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Denylist terms match case-insensitively, as a whole word: `hermes` does not match `thermesh`. */
export function denylistRules(terms: readonly string[]): Rule[] {
  return terms.map((term) => ({
    id: "private-term",
    re: new RegExp(`(?<![A-Za-z0-9])${escapeRegExp(term)}(?![A-Za-z0-9])`, "i"),
  }));
}

/** The denylist file's terms: one per line, `#` starts a comment. */
export function parseDenylist(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.replace(/#.*/, "").trim())
    .filter((l) => l !== "");
}

function mask(s: string): string {
  return s.length <= 6 ? `${s.slice(0, 1)}…` : `${s.slice(0, 4)}…${s.slice(-2)}`;
}

/** Everything sensitive in `lines`, plus files that should not be committed at all. */
export function scan(lines: readonly AddedLine[], denylist: readonly string[]): Finding[] {
  const privateRules = denylistRules(denylist);
  const rules = [...SECRET_RULES, ...privateRules];
  const findings: Finding[] = [];
  const seenFiles = new Set<string>();
  for (const l of lines) {
    if (!seenFiles.has(`${l.commit ?? ""}:${l.file}`)) {
      seenFiles.add(`${l.commit ?? ""}:${l.file}`);
      for (const rule of SECRET_FILES) {
        if (rule.re.test(l.file)) findings.push({ file: l.file, line: 0, commit: l.commit, rule: rule.id, masked: l.file });
      }
      for (const rule of privateRules) {
        const m = rule.re.exec(l.file);
        if (m !== null) findings.push({ file: l.file, line: 0, commit: l.commit, rule: rule.id, masked: mask(m[0]) });
      }
    }
    if (l.text.includes(ALLOW_MARKER)) continue;
    for (const rule of rules) {
      const m = rule.re.exec(l.text);
      if (m !== null) findings.push({ file: l.file, line: l.line, commit: l.commit, rule: rule.id, masked: mask(m[0]) });
    }
  }
  return findings;
}

/**
 * The added lines of `git diff -U0` or `git log -p -U0` output, plus one
 * empty entry (line 0) per added or changed file, empty and binary ones included. For a log, pass
 * `--format=commit %H` so each commit's lines carry its id.
 */
export function addedLines(diff: string): AddedLine[] {
  const out: AddedLine[] = [];
  let commit: string | null = null;
  let file: string | null = null;
  let next = 0;
  // Inside a hunk every `+` line is content, even one that reads like a `+++ ` header.
  let inHunk = false;
  for (const raw of diff.split("\n")) {
    if (raw.startsWith("commit ")) {
      commit = raw.slice("commit ".length).trim();
      inHunk = false;
    } else if (raw.startsWith("diff --git ")) {
      inHunk = false;
      // Every changed file has this header, even an empty or binary one with no
      // hunks, so its entry (line 0, no text) puts the name before the file rules.
      const at = raw.lastIndexOf(" b/");
      if (at !== -1) out.push({ file: raw.slice(at + 3), line: 0, text: "", commit });
    } else if (!inHunk && raw.startsWith("deleted file mode")) {
      out.pop(); // A deleted file is not being added.
    } else if (!inHunk && raw.startsWith("+++ ")) {
      const path = raw.slice(4);
      file = path === "/dev/null" ? null : path.replace(/^b\//, "");
    } else if (raw.startsWith("@@")) {
      inHunk = true;
      const m = /\+(\d+)/.exec(raw);
      next = m?.[1] === undefined ? 0 : Number(m[1]);
    } else if (raw.startsWith("+") && file !== null) {
      out.push({ file, line: next, text: raw.slice(1), commit });
      next += 1;
    }
  }
  return out;
}
