import type { Project, ProjectRule, Session } from "../types";

/** "island": unnamed, so it follows its repo's cluster. */
export type Placement =
  | { kind: "rule"; projectId: string }
  | { kind: "guess"; projectId: string }
  | { kind: "unplaced" }
  | { kind: "island" };

/** Lower-cased, minus a trailing `-x`, then a trailing `-<digits>`. */
export function nameKey(sessionName: string): string {
  return sessionName.toLowerCase().replace(/-x$/, "").replace(/-\d+$/, "");
}

/** The repo's folder name from its path; `Project.name` carries her island renames. */
export function repoFolder(p: Project): string {
  const parts = p.path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

/** Whether `prefix` ends at a hyphen (or the end) of the lower-cased name. */
const hasPrefix = (lower: string, prefix: string): boolean => prefix !== "" && `${lower}-`.startsWith(prefix.toLowerCase());

export const firstWord = (key: string): string => key.split("-")[0] ?? key;

export function placeSession(
  s: Session,
  folder: string,
  rules: readonly ProjectRule[],
  liveKeys: ReadonlyMap<string, number>,
  previous: Placement | undefined,
): Placement {
  if (s.sessionName === null) return { kind: "island" };
  const lower = s.sessionName.toLowerCase();

  let best: ProjectRule | undefined;
  let bestLen = 0;
  for (const r of rules) {
    for (const p of r.prefixes) {
      if (hasPrefix(lower, p) && p.length > bestLen) {
        best = r;
        bestLen = p.length;
      }
    }
  }
  if (best) return { kind: "rule", projectId: best.id };
  const contained = rules.find((r) => r.contains.some((c) => c !== "" && lower.includes(c.toLowerCase())));
  if (contained) return { kind: "rule", projectId: contained.id };

  if (previous?.kind === "guess") return previous;
  const f = folder.toLowerCase();
  if (f !== "" && hasPrefix(lower, `${f}-`)) return { kind: "guess", projectId: f };
  if (s.sessionNameSource === "auto") return { kind: "island" };
  const word = firstWord(nameKey(s.sessionName));
  if ((liveKeys.get(word) ?? 0) >= 2) return { kind: "guess", projectId: word };
  return { kind: "unplaced" };
}
