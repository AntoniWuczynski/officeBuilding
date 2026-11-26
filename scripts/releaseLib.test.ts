import { describe, expect, it } from "vitest";
import {
  assetName,
  builtDmgName,
  ghReleaseCommand,
  RESTORE_COMMAND,
  assetUrl,
  buildLatestJson,
  bumpCargoToml,
  bumpPackageJson,
  compareSemver,
  parseSemver,
  readPackageVersion,
  refusal,
  releaseNotes,
  updaterPlatform,
} from "./releaseLib.ts";
import type { Semver } from "./releaseLib.ts";

const OK = { next: "0.2.0", current: "0.1.0", dirty: false, tagExists: false };

function v(text: string): Semver {
  const parsed = parseSemver(text);
  if (parsed === null) throw new Error(text);
  return parsed;
}

describe("parseSemver", () => {
  it("reads a plain version", () => {
    expect(parseSemver("12.0.3")).toEqual({ major: 12, minor: 0, patch: 3 });
  });
  it.each(["v1.2.3", "1.2", "1.2.3.4", "1.2.3-rc.1", "1.2.3+build", "01.2.3", "1.2.x", "", " 1.2.3"])("refuses %j", (text) => {
    expect(parseSemver(text)).toBeNull();
  });
});

describe("compareSemver", () => {
  it("compares numbers, not text", () => {
    expect(compareSemver(v("0.10.0"), v("0.9.0"))).toBeGreaterThan(0);
    expect(compareSemver(v("1.0.0"), v("0.99.99"))).toBeGreaterThan(0);
    expect(compareSemver(v("0.1.2"), v("0.1.10"))).toBeLessThan(0);
    expect(compareSemver(v("1.2.3"), v("1.2.3"))).toBe(0);
  });
});

describe("refusal", () => {
  it("accepts a higher plain version on a clean tree", () => {
    expect(refusal(OK)).toBeNull();
  });
  it("refuses a version that is not plain semver", () => {
    expect(refusal({ ...OK, next: "v0.2.0" })).toMatch(/not a plain version/);
  });
  it("refuses an equal or lower version", () => {
    expect(refusal({ ...OK, next: "0.1.0" })).toMatch(/not higher/);
    expect(refusal({ ...OK, next: "0.0.9" })).toMatch(/not higher/);
  });
  it("refuses a dirty tree", () => {
    expect(refusal({ ...OK, dirty: true })).toMatch(/uncommitted/);
  });
  it("refuses an existing tag", () => {
    expect(refusal({ ...OK, tagExists: true })).toMatch(/v0\.2\.0 already exists/);
  });
  it("refuses when the current version is not plain", () => {
    expect(refusal({ ...OK, current: "0.1.0-dev" })).toMatch(/current version/);
  });
});

describe("file bumps", () => {
  const pkg = '{\n  "name": "x",\n  "version": "0.1.0",\n  "dependencies": { "a": "1.0.0" }\n}\n';
  it("bumps package.json and nothing else", () => {
    expect(bumpPackageJson(pkg, "0.2.0")).toBe(pkg.replace('"version": "0.1.0"', '"version": "0.2.0"'));
    expect(readPackageVersion(pkg)).toBe("0.1.0");
  });
  it("bumps the [package] version, not a later one", () => {
    const toml = '[package]\nname = "x"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nserde = { version = "1" }\nversion = "9.9.9"\n';
    expect(bumpCargoToml(toml, "0.2.0")).toBe(toml.replace('version = "0.1.0"', 'version = "0.2.0"'));
  });
  it("bumps a [package] table at the very end", () => {
    expect(bumpCargoToml('[package]\nversion = "0.1.0"', "0.2.0")).toBe('[package]\nversion = "0.2.0"');
  });
  it("throws when there is nothing to bump", () => {
    expect(() => bumpPackageJson("{}", "1.0.0")).toThrow(/no version/);
    expect(() => bumpCargoToml("[lib]\nname = 'x'\n", "1.0.0")).toThrow(/no \[package\]/);
    expect(() => bumpCargoToml("[package]\nname = 'x'\n[dependencies]\nversion = \"1\"\n", "1.0.0")).toThrow(/no version/);
  });
});

describe("updater manifest", () => {
  it("maps this machine to the updater's platform key", () => {
    expect(updaterPlatform("darwin", "arm64")).toBe("darwin-aarch64");
    expect(updaterPlatform("darwin", "x64")).toBe("darwin-x86_64");
    expect(updaterPlatform("linux", "x64")).toBeNull();
    expect(updaterPlatform("darwin", "ia32")).toBeNull();
  });
  it("builds latest.json in the static format", () => {
    const name = assetName("0.2.0", "aarch64", "app.tar.gz");
    const json = buildLatestJson("0.2.0", "- one", new Date("2026-11-26T10:00:00Z"), {
      "darwin-aarch64": { signature: "SIG", url: assetUrl("owner/repo", "v0.2.0", name) },
    });
    expect(JSON.parse(JSON.stringify(json))).toEqual({
      version: "0.2.0",
      notes: "- one",
      pub_date: "2026-11-26T10:00:00.000Z",
      platforms: { "darwin-aarch64": { signature: "SIG", url: "https://github.com/owner/repo/releases/download/v0.2.0/Office-Building_0.2.0_aarch64.app.tar.gz" } },
    });
  });
  it("names assets without spaces", () => {
    expect(assetName("0.2.0", "aarch64", "dmg")).toBe("Office-Building_0.2.0_aarch64.dmg");
    expect(assetName("0.2.0", "aarch64", "app.tar.gz.sig")).not.toMatch(/\s/);
  });
});

describe("releaseNotes", () => {
  it("lists the subjects", () => {
    expect(releaseNotes(["Fix a", " ", "Add b"])).toBe("- Fix a\n- Add b");
  });
  it("says so when there are none", () => {
    expect(releaseNotes([])).toBe("No changes recorded.");
  });
});

describe("builtDmgName", () => {
  it("is the exact file Tauri writes for this version and arch, so an older image is never picked", () => {
    expect(builtDmgName("Office Building", "0.2.0", "aarch64")).toBe("Office Building_0.2.0_aarch64.dmg");
    expect(builtDmgName("Office Building", "0.2.0", "aarch64")).not.toBe(builtDmgName("Office Building", "0.1.0", "aarch64"));
  });
});

describe("ghReleaseCommand", () => {
  it("prints a command that can be rerun, quoting paths with spaces", () => {
    expect(ghReleaseCommand("o/r", "v0.2.0", "out/notes.md", ["out/a.tar.gz", "my dir/b's.dmg"])).toBe(
      "gh release create v0.2.0 --repo o/r --title v0.2.0 --notes-file out/notes.md out/a.tar.gz 'my dir/b'\\''s.dmg'",
    );
  });
});

describe("RESTORE_COMMAND", () => {
  it("restores the three files the release rewrites", () => {
    expect(RESTORE_COMMAND).toBe("git checkout -- package.json src-tauri/Cargo.toml src-tauri/Cargo.lock");
  });
});
