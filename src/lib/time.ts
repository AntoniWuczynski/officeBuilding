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

const WEEKDAY = new Intl.DateTimeFormat("en-GB", { weekday: "short" });

/** When a usage window resets: "in 3 h" within a day, otherwise the weekday ("Fri"). */
export function resetsLabel(iso: string, now: Date): string {
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return "at an unknown time";
  const minutes = Math.round((at - now.getTime()) / 60_000);
  if (minutes <= 0) return "now";
  if (minutes < 60) return `in ${minutes} min`;
  if (minutes < 24 * 60) return `in ${Math.round(minutes / 60)} h`;
  return WEEKDAY.format(at);
}
