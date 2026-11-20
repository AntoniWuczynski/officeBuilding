/** Top-level selectors of a stylesheet, comments stripped. An at-rule's prelude counts as one, so a scope check rejects it. */
export function selectors(css: string): string[] {
  const body = css.replace(/\/\*[\s\S]*?\*\//g, "");
  return [...body.matchAll(/([^{}]+)\{/g)].flatMap((m) => (m[1] ?? "").split(",").map((s) => s.trim())).filter((s) => s !== "");
}
