/** 1234 -> "1.2k", 3_400_000 -> "3.4M". */
export function compactNumber(value: number): string {
  if (value < 1000) return String(Math.round(value));
  const units = [
    [1e9, "B"],
    [1e6, "M"],
    [1e3, "k"],
  ] as const;
  for (const [size, suffix] of units) {
    if (value >= size) {
      const scaled = value / size;
      return `${scaled >= 100 ? Math.round(scaled) : scaled.toFixed(1).replace(/\.0$/, "")}${suffix}`;
    }
  }
  return String(value);
}

/** US dollars with sensible precision: "$0.42", "$12.30", "$148". */
export function dollars(value: number): string {
  if (value >= 100) return `$${Math.round(value)}`;
  return `$${value.toFixed(2)}`;
}

/** Time until a moment, short: "2h 14m", "38m", "4d 3h", "now". */
export function countdown(untilMs: number, nowMs: number): string {
  const minutes = Math.max(0, Math.round((untilMs - nowMs) / 60_000));
  if (minutes < 1) return "now";
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const rest = minutes % 60;
  if (days > 0) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (hours > 0) return rest > 0 ? `${hours}h ${rest}m` : `${hours}h`;
  return `${rest}m`;
}

/** How long ago, in words: "just now", "5 min ago", "3 h ago", "2 days ago". */
export function ago(thenMs: number, nowMs: number): string {
  const minutes = Math.floor((nowMs - thenMs) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

/** Colour for a usage percentage: calm, then warning at 80%, then critical at 95%. */
export function usageColor(percent: number): string {
  if (percent >= 95) return "var(--bad)";
  if (percent >= 80) return "var(--warn)";
  return "var(--accent)";
}
