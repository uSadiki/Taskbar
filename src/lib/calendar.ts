/** First day of the week for a locale: 0 = Sunday .. 6 = Saturday. Defaults to Monday. */
export function weekStartFor(locale?: string): number {
  try {
    const loc = new Intl.Locale(locale ?? navigator.language) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number };
      weekInfo?: { firstDay: number };
    };
    const first = loc.getWeekInfo?.().firstDay ?? loc.weekInfo?.firstDay;
    // Intl uses 1 = Monday .. 7 = Sunday.
    if (typeof first === "number") return first % 7;
  } catch {
    // Unknown locale: fall through to Monday.
  }
  return 1;
}

/**
 * The six weeks shown for a month: every day of the month plus the days of
 * neighbouring months that complete the first and last week.
 */
export function monthGrid(year: number, month: number, weekStart: number): Date[][] {
  const first = new Date(year, month, 1);
  const lead = (first.getDay() - weekStart + 7) % 7;
  const weeks: Date[][] = [];
  for (let week = 0; week < 6; week++) {
    const days: Date[] = [];
    for (let day = 0; day < 7; day++) {
      days.push(new Date(year, month, 1 - lead + week * 7 + day));
    }
    weeks.push(days);
  }
  return weeks;
}

/** ISO 8601 week number (weeks start on Monday; week 1 contains 4 January). */
export function isoWeek(date: Date): number {
  const d = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
  const weekday = d.getUTCDay() || 7;
  d.setUTCDate(d.getUTCDate() + 4 - weekday); // the Thursday of this week
  const yearStart = Date.UTC(d.getUTCFullYear(), 0, 1);
  return Math.ceil(((d.getTime() - yearStart) / 86_400_000 + 1) / 7);
}

export function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
  );
}

/** Short weekday names in display order, starting from `weekStart`. */
export function weekdayNames(weekStart: number, locale?: string): string[] {
  const format = new Intl.DateTimeFormat(locale, { weekday: "short" });
  // 5 January 2025 is a Sunday.
  return Array.from({ length: 7 }, (_, i) => format.format(new Date(2025, 0, 5 + ((weekStart + i) % 7))));
}
