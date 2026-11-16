/** Milliseconds from `now` until the next minute begins (always 1..60000). */
export function msUntilNextMinute(now: Date): number {
  const elapsed = now.getSeconds() * 1000 + now.getMilliseconds();
  return 60_000 - elapsed;
}

/** Clock text in the user's locale, for example "14:05" or "2:05 PM". */
export function formatTime(now: Date, locale?: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }).format(now);
}

/** Short date in the user's locale, for example "man. 16. nov." or "Mon, Nov 16". */
export function formatDate(now: Date, locale?: string): string {
  return new Intl.DateTimeFormat(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
  }).format(now);
}
