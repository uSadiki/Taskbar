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

/** A length of time in words: "3 h 10 min", "40 min", "under a minute". */
export function formatDuration(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  if (minutes < 1) return "under a minute";
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) return `${rest} min`;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}

/** A playback position: "3:07", "1:02:45". */
export function formatPlayback(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${seconds}` : `${minutes}:${seconds}`;
}

/**
 * Where playback is now, from the last reported position. Players report a
 * position only now and then; while playing, it advances with the clock.
 */
export function playbackPosition(
  state: { positionMs: number; durationMs: number; updatedAtMs: number; playing: boolean },
  nowMs: number,
): number {
  const elapsed = state.playing && state.updatedAtMs > 0 ? Math.max(0, nowMs - state.updatedAtMs) : 0;
  const position = state.positionMs + elapsed;
  return state.durationMs > 0 ? Math.min(position, state.durationMs) : position;
}
