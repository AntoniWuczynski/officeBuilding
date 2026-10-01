/**
 * Dropped file paths as Terminal.app types them: ASCII shell metacharacters
 * backslash-escaped, paths separated by spaces. Claude Code reads a pasted
 * image path in this form as an attached image.
 */
export function shellPaths(paths: readonly string[]): string {
  return paths.map((p) => p.replace(/[^\w\-./,+@%:=~\u{80}-\u{10FFFF}]/gu, "\\$&")).join(" ");
}
