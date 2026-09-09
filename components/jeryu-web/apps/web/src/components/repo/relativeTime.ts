// relativeTime.ts — shared relative-time formatter for repo surfaces.
//
// Best-effort relative time. Falls back to the raw timestamp if Intl
// RelativeTimeFormat is unavailable (e.g. a JSDOM test environment that
// does not implement it) or the input does not parse.

export function relativeTime(iso: string): string {
  try {
    const then = new Date(iso).getTime();
    const now = Date.now();
    if (!Number.isFinite(then)) return iso;
    const deltaSeconds = Math.round((then - now) / 1000);
    const abs = Math.abs(deltaSeconds);
    const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
    if (abs < 60) return rtf.format(deltaSeconds, 'second');
    if (abs < 3600) return rtf.format(Math.round(deltaSeconds / 60), 'minute');
    if (abs < 86400) {
      return rtf.format(Math.round(deltaSeconds / 3600), 'hour');
    }
    if (abs < 30 * 86400) {
      return rtf.format(Math.round(deltaSeconds / 86400), 'day');
    }
    return rtf.format(Math.round(deltaSeconds / (30 * 86400)), 'month');
  } catch {
    return iso;
  }
}
