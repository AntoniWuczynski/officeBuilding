/** The words of whatever was thrown, for an error message. */
export function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
