import { useEffect, useState } from "react";
import { formatDate, formatTime, msUntilNextMinute } from "../lib/time";

/** Time and date at the end of the dock. Wakes once a minute, on the minute. */
export function Clock() {
  const [now, setNow] = useState(() => new Date());

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
    <div className="dock-clock" role="timer" aria-label="Clock">
      <span className="dock-clock-time">{formatTime(now)}</span>
      <span className="dock-clock-date">{formatDate(now)}</span>
    </div>
  );
}
