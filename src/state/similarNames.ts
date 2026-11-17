const words = (s: string): string[] => s.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);

function distance(a: string, b: string): number {
  let row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const next = [i];
    for (let j = 1; j <= b.length; j++) next[j] = Math.min((row[j] ?? 0) + 1, (next[j - 1] ?? 0) + 1, (row[j - 1] ?? 0) + (a[i - 1] === b[j - 1] ? 0 : 1));
    row = next;
  }
  return row[b.length] ?? 0;
}

/** Lower is closer; null is not similar. */
function score(a: string, b: string): number | null {
  const sa = words(a).join(""), sb = words(b).join("");
  if (sa === "" || sb === "") return null;
  if (sa === sb) return 0;
  if (sa === `${sb}s` || sb === `${sa}s`) return 1;
  const [short, long] = sa.length <= sb.length ? [sa, sb] : [sb, sa];
  if (short.length >= 4 && long.startsWith(short)) return 2;
  const wb = new Set(words(b));
  if (words(a).some((w) => w.length >= 3 && wb.has(w))) return 3;
  if (short.length >= 4 && distance(sa, sb) <= (short.length >= 6 ? 2 : 1)) return 4;
  return null;
}

/** Existing names close to `input`, best first; the identical name is left out. */
export function similarNames(input: string, existing: readonly string[]): string[] {
  const hits: { name: string; score: number }[] = [];
  for (const name of existing) {
    const sc = name === input ? null : score(input, name);
    if (sc !== null) hits.push({ name, score: sc });
  }
  return hits.sort((x, y) => x.score - y.score).map((h) => h.name);
}
