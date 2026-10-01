import { describe, expect, it } from "vitest";
import { addedLines, parseDenylist, scan } from "./sensitive.ts";
import type { AddedLine } from "./sensitive.ts";

// Fixture secrets are assembled at run time, so this file never holds one and
// the hook it tests does not block it.
const j = (...parts: string[]): string => parts.join("");
const FAKE = {
  anthropic: j("sk-", "ant-", "api03-", "Ab3".repeat(10)),
  openai: j("sk-", "proj-", "Q7x".repeat(14)),
  github: j("gh", "p_", "a1B2".repeat(10)),
  aws: j("AK", "IA", "ABCDEFGHIJ234567"),
  slack: j("xo", "xb-", "1234567890-abcdefghij"),
  google: j("AI", "za", "Sy".repeat(17), "x"),
  stripe: j("sk", "_live_", "a1".repeat(12)),
  pem: j("-----BEGIN ", "RSA PRIVATE", " KEY-----"),
  jwt: j("ey", "JhbGciOiJIUzI1NiJ9.", "ey", "JzdWIiOiIxMjM0NTY3ODkwIn0.", "abcdefghijklmnop"),
  home: j("/Us", "ers/", "jdoe/code/app"),
};

function lines(...texts: string[]): AddedLine[] {
  return texts.map((text, i) => ({ file: "src/a.ts", line: i + 1, text, commit: null }));
}
const rulesHit = (texts: string[], deny: string[] = []): string[] => scan(lines(...texts), deny).map((f) => f.rule);

describe("secret rules", () => {
  it("catch each secret shape, bare or inside quoted strings", () => {
    const cases: [string, string][] = [
      ["anthropic-key", FAKE.anthropic],
      ["openai-key", FAKE.openai],
      ["github-token", FAKE.github],
      ["aws-access-key", FAKE.aws],
      ["slack-token", FAKE.slack],
      ["google-api-key", FAKE.google],
      ["stripe-key", FAKE.stripe],
      ["private-key", FAKE.pem],
      ["jwt", FAKE.jwt],
      ["home-path", FAKE.home],
    ];
    for (const [rule, secret] of cases) {
      expect(rulesHit([secret]), rule).toContain(rule);
      expect(rulesHit([`const k = "${secret}";`]), `${rule} in double quotes`).toContain(rule);
      expect(rulesHit([`key: '${secret}'`]), `${rule} in single quotes`).toContain(rule);
    }
    expect(rulesHit([j('{"api_key": "', "Zq9".repeat(6), '"}')])).toContain("credential-assignment");
    expect(rulesHit([j("PASSWORD='", "hunter2hunter2", "'")])).toContain("credential-assignment");
  });

  it("leave placeholders, short or embedded look-alikes and fictional paths alone", () => {
    expect(
      rulesHit([
        "Set ANTHROPIC_API_KEY=sk-ant-... in your .env",
        j("task-", "ant-", "api03-", "x".repeat(30)),
        "risk-assessment-for-the-quarterly-planning-process-and-beyond",
        "AKIA is the prefix AWS uses",
        "the token: process.env.GITHUB_TOKEN",
        'password: ""',
        'secret = "short"',
        "/Users/me/code/app and /Users/runner/work",
        "eyJ alone is not a token",
      ]),
    ).toEqual([]);
  });

  it("skip a reviewed false positive marked on its line", () => {
    expect(rulesHit([`${FAKE.home} // sensitive-ok: example path in the docs`])).toEqual([]);
  });

  it("mask what matched in the report", () => {
    const [finding] = scan(lines(FAKE.anthropic), []);
    expect(finding?.masked).not.toContain(FAKE.anthropic.slice(8));
    expect(finding?.masked.startsWith("sk-a")).toBe(true);
  });
});

describe("denylist", () => {
  const deny = parseDenylist("# private project names\nsecretproject\n\nacme-labs  # client\n");

  it("reads one term per line and drops comments and blanks", () => {
    expect(deny).toEqual(["secretproject", "acme-labs"]);
  });

  it("matches whole words in any case, inside quotes too", () => {
    expect(rulesHit(["Ported from SecretProject's broker"], deny)).toEqual(["private-term"]);
    expect(rulesHit(['import x from "acme-labs/kit"'], deny)).toEqual(["private-term"]);
    expect(rulesHit(["'secretproject'"], deny)).toEqual(["private-term"]);
  });

  it("does not match a term buried inside a longer word", () => {
    expect(rulesHit(["topsecretprojectx", "acme-labsuite"], deny)).toEqual([]);
  });

  it("checks file names as well as their text", () => {
    const named: AddedLine[] = [{ file: "docs/secretproject-notes.md", line: 0, text: "", commit: null }];
    expect(scan(named, deny).map((f) => f.rule)).toEqual(["private-term"]);
  });
});

describe("files that should never be committed", () => {
  const fileRules = (file: string): string[] => scan([{ file, line: 0, text: "", commit: null }], []).map((f) => f.rule);

  it("flag env files, keys and keystores", () => {
    expect(fileRules(".env")).toEqual(["env-file"]);
    expect(fileRules("src-tauri/.env.local")).toEqual(["env-file"]);
    expect(fileRules("certs/server.pem")).toEqual(["key-file"]);
    expect(fileRules("id_ed25519")).toEqual(["ssh-key-file"]);
  });

  it("allow the example env file and look-alike names", () => {
    expect(fileRules(".env.example")).toEqual([]);
    expect(fileRules("src/environment.ts")).toEqual([]);
    expect(fileRules("docs/keys.md")).toEqual([]);
  });
});

describe("addedLines", () => {
  it("numbers added lines from the hunk header and ignores removed ones", () => {
    const diff = [
      "diff --git a/src/a.ts b/src/a.ts",
      "--- a/src/a.ts",
      "+++ b/src/a.ts",
      "@@ -3,2 +10,2 @@",
      "-old line",
      "+first",
      "+second",
      "diff --git a/gone.ts b/gone.ts",
      "deleted file mode 100644",
      "--- a/gone.ts",
      "+++ /dev/null",
      "@@ -1 +0,0 @@",
      "-bye",
    ].join("\n");
    expect(addedLines(diff)).toEqual([
      { file: "src/a.ts", line: 0, text: "", commit: null },
      { file: "src/a.ts", line: 10, text: "first", commit: null },
      { file: "src/a.ts", line: 11, text: "second", commit: null },
    ]);
  });

  it("tags lines with their commit and notices binary files", () => {
    const log = ["commit abc123", "diff --git a/k.p12 b/k.p12", "new file mode 100644", "Binary files /dev/null and b/k.p12 differ"].join("\n");
    expect(addedLines(log)).toEqual([{ file: "k.p12", line: 0, text: "", commit: "abc123" }]);
    expect(scan(addedLines(log), []).map((f) => f.rule)).toEqual(["key-file"]);
  });

  it("does not take a deleted binary file for an added one", () => {
    const log = ["diff --git a/k.p12 b/k.p12", "deleted file mode 100644", "Binary files a/k.p12 and /dev/null differ"].join("\n");
    expect(addedLines(log)).toEqual([]);
  });

  it("reads an added line that starts with ++ as content, not a file header", () => {
    const diff = ["diff --git a/a.md b/a.md", "+++ b/a.md", "@@ -0,0 +1 @@", "+++ b/secret path", "@@ -5,0 +7 @@", "+x"].join("\n");
    expect(addedLines(diff).map((l) => [l.file, l.line, l.text])).toEqual([
      ["a.md", 0, ""],
      ["a.md", 1, "++ b/secret path"],
      ["a.md", 7, "x"],
    ]);
  });

  it("notices a new empty file, which has no hunks at all", () => {
    const diff = ["diff --git a/.env b/.env", "new file mode 100644", "index 0000000..e69de29"].join("\n");
    expect(scan(addedLines(diff), []).map((f) => f.rule)).toEqual(["env-file"]);
  });
});
