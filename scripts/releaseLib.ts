// The pure parts of scripts/release.ts: version rules, file edits as string
// transforms and the updater's latest.json. Nothing here touches the disk or git.

export interface Semver {
  readonly major: number;
  readonly minor: number;
  readonly patch: number;
}

const PLAIN_SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

/** `1.2.3` as numbers, or null for anything else: a leading `v`, a pre-release or build suffix, a missing part. */
export function parseSemver(text: string): Semver | null {
  const match = PLAIN_SEMVER.exec(text);
  if (match === null) return null;
  const [, major, minor, patch] = match;
  if (major === undefined || minor === undefined || patch === undefined) return null;
  return { major: Number(major), minor: Number(minor), patch: Number(patch) };
}

/** Negative when `a` is lower than `b`, zero when equal, positive when higher. */
export function compareSemver(a: Semver, b: Semver): number {
  return a.major - b.major || a.minor - b.minor || a.patch - b.patch;
}

export interface ReleaseState {
  readonly next: string;
  readonly current: string;
  readonly dirty: boolean;
  readonly tagExists: boolean;
}

/** The reason a release cannot be cut, or null when it can. */
export function refusal({ next, current, dirty, tagExists }: ReleaseState): string | null {
  const wanted = parseSemver(next);
  if (wanted === null) return `"${next}" is not a plain version like 1.2.3 (no leading v, no suffix).`;
  const installed = parseSemver(current);
  if (installed === null) return `The current version "${current}" is not a plain version like 1.2.3.`;
  if (compareSemver(wanted, installed) <= 0) return `${next} is not higher than the current version ${current}.`;
  if (dirty) return "The working tree has uncommitted changes. Commit or stash them first.";
  if (tagExists) return `The tag v${next} already exists.`;
  return null;
}

function replaceOnce(text: string, pattern: RegExp, version: string, what: string): string {
  if (!pattern.test(text)) throw new Error(`${what} has no version to replace.`);
  return text.replace(pattern, (_all, before: string, after: string) => `${before}${version}${after}`);
}

/** package.json with its top-level `version` replaced, everything else byte for byte. */
export function bumpPackageJson(text: string, version: string): string {
  return replaceOnce(text, /^(\s*"version":\s*")[^"]*(")/m, version, "package.json");
}

/** Cargo.toml with the `version` of its `[package]` table replaced. */
export function bumpCargoToml(text: string, version: string): string {
  const start = text.search(/^\[package\]\s*$/m);
  if (start < 0) throw new Error("Cargo.toml has no [package] table.");
  const rest = text.slice(start);
  const end = rest.slice(1).search(/^\[/m);
  const table = end < 0 ? rest : rest.slice(0, end + 1);
  const bumped = replaceOnce(table, /^(version\s*=\s*")[^"]*(")/m, version, "Cargo.toml [package]");
  return text.slice(0, start) + bumped + rest.slice(table.length);
}

/** The version in a package.json text. */
export function readPackageVersion(text: string): string {
  const match = /^\s*"version":\s*"([^"]*)"/m.exec(text);
  if (match?.[1] === undefined) throw new Error("package.json has no version.");
  return match[1];
}

/** The platform key the updater uses for this machine, or null when the app is not built for it. */
export function updaterPlatform(platform: string, arch: string): string | null {
  if (platform !== "darwin") return null;
  if (arch === "arm64") return "darwin-aarch64";
  if (arch === "x64") return "darwin-x86_64";
  return null;
}

export interface PlatformEntry {
  readonly signature: string;
  readonly url: string;
}

export interface LatestJson {
  readonly version: string;
  readonly notes: string;
  readonly pub_date: string;
  readonly platforms: Readonly<Record<string, PlatformEntry>>;
}

/** The updater's static manifest (https://v2.tauri.app/plugin/updater/): `version`, `notes`, `pub_date` and a signature and url for each platform. */
export function buildLatestJson(version: string, notes: string, pubDate: Date, platforms: Readonly<Record<string, PlatformEntry>>): LatestJson {
  return { version, notes, pub_date: pubDate.toISOString(), platforms };
}

/** What a release's assets are called. GitHub turns spaces in an upload's name into dots, so none are used. */
export function assetName(version: string, arch: string, extension: "app.tar.gz" | "app.tar.gz.sig" | "dmg"): string {
  return `Office-Building_${version}_${arch}.${extension}`;
}

/** The disk image's name as Tauri builds it, `<productName>_<version>_<arch>.dmg`. Older versions' images stay in the folder, so the build's own is looked up by this exact name. */
export function builtDmgName(productName: string, version: string, arch: string): string {
  return `${productName}_${version}_${arch}.dmg`;
}

/** Where GitHub serves a release asset from. */
export function assetUrl(repo: string, tag: string, name: string): string {
  return `https://github.com/${repo}/releases/download/${tag}/${name}`;
}

/** Release notes from the commit subjects since the last release. */
export function releaseNotes(subjects: readonly string[]): string {
  const lines = subjects.map((s) => s.trim()).filter((s) => s !== "");
  return lines.length === 0 ? "No changes recorded." : lines.map((s) => `- ${s}`).join("\n");
}

/** The `gh release create` command that publishes the release by hand, quoted for a shell. */
export function ghReleaseCommand(repo: string, tag: string, notesFile: string, assets: readonly string[]): string {
  const quote = (text: string): string => `'${text.replaceAll("'", "'\\''")}'`;
  return ["gh", "release", "create", tag, "--repo", repo, "--title", tag, "--notes-file", notesFile, ...assets]
    .map((part) => (/^[\w@%+=:,./-]+$/.test(part) ? part : quote(part)))
    .join(" ");
}

/** What to run to put the version files back after a release step failed before the commit was made. */
export const RESTORE_COMMAND = "git checkout -- package.json src-tauri/Cargo.toml src-tauri/Cargo.lock";
