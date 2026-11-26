// Cuts a release: `pnpm release 0.2.0`, from a clean checkout.
//   bumps package.json and Cargo.toml, commits "Release v0.2.0", tags v0.2.0,
//   builds and signs the app, writes latest.json for the in-app updater, pushes
//   the commit and tag, and publishes the GitHub release with the assets.
// The version rules and file edits live in releaseLib.ts, where they are tested.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import {
  assetName,
  assetUrl,
  buildLatestJson,
  bumpCargoToml,
  bumpPackageJson,
  readPackageVersion,
  refusal,
  releaseNotes,
  updaterPlatform,
  type PlatformEntry,
} from "./releaseLib.ts";

const REPO = "AntoniWuczynski/officeBuilding";
const CARGO_CRATE = "office-building";
const DEFAULT_KEY = join(homedir(), ".tauri", "office-building.key");

function run(command: string, args: readonly string[], env: NodeJS.ProcessEnv = process.env): string {
  return execFileSync(command, args, { encoding: "utf8", env, stdio: ["ignore", "pipe", "inherit"] });
}

function runVisible(command: string, args: readonly string[], env: NodeJS.ProcessEnv = process.env): void {
  execFileSync(command, args, { env, stdio: "inherit" });
}

function fail(text: string): never {
  console.error(text);
  process.exit(1);
}

function tagExists(tag: string): boolean {
  return run("git", ["tag", "--list", tag]).trim() !== "";
}

/** The commit subjects since the newest tag, or the last twenty when nothing is tagged yet. */
function subjectsSinceLastTag(): string[] {
  const latest = run("git", ["tag", "--list", "v*", "--sort=-v:refname"]).split("\n")[0]?.trim() ?? "";
  const range = latest === "" ? ["-n", "20"] : [`${latest}..HEAD`];
  return run("git", ["log", "--no-merges", "--format=%s", ...range]).split("\n");
}

function signingKey(): string {
  const path = process.env["TAURI_SIGNING_PRIVATE_KEY_PATH"] ?? DEFAULT_KEY;
  if (!existsSync(path)) fail(`No signing key at ${path}. Set TAURI_SIGNING_PRIVATE_KEY_PATH to the key's file.`);
  return readFileSync(path, "utf8");
}

function findFile(dir: string, suffix: string): string {
  const name = readdirSync(dir).find((f) => f.endsWith(suffix));
  if (name === undefined) fail(`The build left no *${suffix} in ${dir}.`);
  return join(dir, name);
}

function main(): void {
  const next = process.argv[2];
  if (next === undefined) fail("Usage: pnpm release <version>, for example pnpm release 0.2.0");
  const tag = `v${next}`;
  const current = readPackageVersion(readFileSync("package.json", "utf8"));
  const problem = refusal({ next, current, dirty: run("git", ["status", "--porcelain"]).trim() !== "", tagExists: tagExists(tag) });
  if (problem !== null) fail(problem);
  const platform = updaterPlatform(process.platform, process.arch);
  if (platform === null) fail(`Cannot build the app on ${process.platform} ${process.arch}.`);
  const key = signingKey();
  const notes = releaseNotes(subjectsSinceLastTag());

  writeFileSync("package.json", bumpPackageJson(readFileSync("package.json", "utf8"), next));
  writeFileSync("src-tauri/Cargo.toml", bumpCargoToml(readFileSync("src-tauri/Cargo.toml", "utf8"), next));
  runVisible("cargo", ["update", "--manifest-path", "src-tauri/Cargo.toml", "-p", CARGO_CRATE, "--offline"]);
  runVisible("git", ["add", "package.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"]);
  runVisible("git", ["commit", "-m", `Release ${tag}`]);
  runVisible("git", ["tag", tag]);

  try {
    runVisible("pnpm", ["tauri", "build"], { ...process.env, TAURI_SIGNING_PRIVATE_KEY: key, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "" });
  } catch (err) {
    console.error(`The build failed. Nothing was pushed. To start over: git tag -d ${tag} && git reset --hard HEAD~1`);
    throw err;
  }

  const arch = platform.endsWith("aarch64") ? "aarch64" : "x86_64";
  const bundle = join("src-tauri", "target", "release", "bundle");
  const out = join("src-tauri", "target", "release", "release-assets", tag);
  mkdirSync(out, { recursive: true });
  const tarball = assetName(next, arch, "app.tar.gz");
  const signature = assetName(next, arch, "app.tar.gz.sig");
  const dmg = assetName(next, arch, "dmg");
  const builtTarball = findFile(join(bundle, "macos"), ".app.tar.gz");
  copyFileSync(builtTarball, join(out, tarball));
  copyFileSync(`${builtTarball}.sig`, join(out, signature));
  copyFileSync(findFile(join(bundle, "dmg"), ".dmg"), join(out, dmg));

  const entries: Record<string, PlatformEntry> = {
    [platform]: { signature: readFileSync(join(out, signature), "utf8").trim(), url: assetUrl(REPO, tag, tarball) },
  };
  const latest = join(out, "latest.json");
  writeFileSync(latest, `${JSON.stringify(buildLatestJson(next, notes, new Date(), entries), null, 2)}\n`);
  const notesFile = join(out, "notes.md");
  writeFileSync(notesFile, `${notes}\n`);

  runVisible("git", ["push", "origin", "HEAD", tag]);
  runVisible("gh", [
    "release", "create", tag, "--repo", REPO, "--title", tag, "--notes-file", notesFile,
    join(out, tarball), join(out, signature), join(out, dmg), latest,
  ]);
  console.log(`Released ${tag}.`);
}

main();
