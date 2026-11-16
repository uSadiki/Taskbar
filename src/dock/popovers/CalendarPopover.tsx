import { useMemo, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { isoWeek, monthGrid, sameDay, weekStartFor, weekdayNames } from "../../lib/calendar";

/** Month calendar opened from the clock. */
export function CalendarPopover() {
  const today = useMemo(() => new Date(), []);
  const [view, setView] = useState({ year: today.getFullYear(), month: today.getMonth() });
  const weekStart = useMemo(() => weekStartFor(), []);
  const names = useMemo(() => weekdayNames(weekStart), [weekStart]);
  const weeks = monthGrid(view.year, view.month, weekStart);

  const step = (delta: number) =>
    setView(({ year, month }) => {
      const next = new Date(year, month + delta, 1);
      return { year: next.getFullYear(), month: next.getMonth() };
    });
  const onCurrentMonth = view.year === today.getFullYear() && view.month === today.getMonth();
  const title = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" }).format(
    new Date(view.year, view.month, 1),
  );
  const fullDate = new Intl.DateTimeFormat(undefined, { dateStyle: "full" }).format(today);

  return (
    <div className="p-4">
      <div className="mb-3 text-[13px] text-[var(--ink-muted)]">{fullDate}</div>
      <div className="mb-2 flex items-center gap-1">
        <div className="flex-1 text-[15px] font-semibold first-letter:uppercase">{title}</div>
        {!onCurrentMonth && (
          <button
            type="button"
            className="glass-button px-2 text-[12px]"
            onClick={() => setView({ year: today.getFullYear(), month: today.getMonth() })}
          >
            Today
          </button>
        )}
        <button type="button" className="glass-button" aria-label="Previous month" onClick={() => step(-1)}>
          <ChevronLeft size={16} aria-hidden />
        </button>
        <button type="button" className="glass-button" aria-label="Next month" onClick={() => step(1)}>
          <ChevronRight size={16} aria-hidden />
        </button>
      </div>

      <table className="w-full border-collapse text-center text-[12.5px] tabular-nums">
        <thead>
          <tr className="text-[var(--ink-faint)]">
            <th className="w-7 pb-1 font-normal" aria-label="Week" />
            {names.map((name) => (
              <th key={name} className="pb-1 font-normal" scope="col">
                {name}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {weeks.map((week) => (
            <tr key={week[0].toISOString()}>
              <td className="text-[11px] text-[var(--ink-faint)]">{isoWeek(week[3])}</td>
              {week.map((day) => {
                const isToday = sameDay(day, today);
                const outside = day.getMonth() !== view.month;
                return (
                  <td key={day.toISOString()} className="p-0">
                    <span
                      aria-current={isToday ? "date" : undefined}
                      className={`mx-auto grid h-8 w-8 place-items-center rounded-full ${
                        isToday
                          ? "bg-[var(--accent)] font-semibold text-[var(--accent-ink)]"
                          : outside
                            ? "text-[var(--ink-faint)]"
                            : ""
                      }`}
                    >
                      {day.getDate()}
                    </span>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
