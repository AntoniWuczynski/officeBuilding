/** True relative time ("4 min ago") between an ISO timestamp and `now`. */
export function relativeTime(iso: string, now: Date): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "at an unknown time";
  const seconds = Math.max(0, Math.round((now.getTime() - then) / 1000));
  if (seconds < 60) return "under a minute ago";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  return days === 1 ? "1 day ago" : `${days} days ago`;
}
