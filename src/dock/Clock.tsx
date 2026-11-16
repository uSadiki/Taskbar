import { useEffect, useState } from "react";
import { formatDate, formatTime, msUntilNextMinute } from "../lib/time";
import { anchorOf, usePopover } from "./popover";

/**
 * Time and date at the end of the dock; opens the calendar. Wakes once a
 * minute, on the minute.
 */
export function Clock() {
  const [now, setNow] = useState(() => new Date());
  const toggle = usePopover((s) => s.toggle);
  const open = usePopover((s) => s.id === "clock");

  useEffect(() => {
    let timer = 0;
    const schedule = () => {
      timer = window.setTimeout(() => {
        setNow(new Date());
        schedule();
      }, msUntilNextMinute(new Date()));
    };
    schedule();
    return () => window.clearTimeout(timer);
  }, []);

  return (
    <button
      type="button"
      className="dock-clock"
      aria-label={`${formatTime(now)}, ${formatDate(now)}. Open calendar`}
      aria-expanded={open}
      onClick={(e) => toggle("clock", anchorOf(e.currentTarget))}
    >
      <span className="dock-clock-time">{formatTime(now)}</span>
      <span className="dock-clock-date">{formatDate(now)}</span>
    </button>
  );
}
