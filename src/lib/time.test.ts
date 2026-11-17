import { describe, expect, it } from "vitest";
import { formatDate, formatTime, msUntilNextMinute } from "./time";

describe("msUntilNextMinute", () => {
  it("returns a full minute exactly on the minute", () => {
    expect(msUntilNextMinute(new Date(2026, 10, 16, 14, 5, 0, 0))).toBe(60_000);
  });

  it("returns the remainder mid-minute", () => {
    expect(msUntilNextMinute(new Date(2026, 10, 16, 14, 5, 42, 250))).toBe(17_750);
  });

  it("never returns zero, so the timer can't spin", () => {
    expect(msUntilNextMinute(new Date(2026, 10, 16, 14, 5, 59, 999))).toBe(1);
  });
});

describe("formatting", () => {
  const date = new Date(2026, 10, 16, 14, 5);

  it("uses a 24-hour clock for Norwegian", () => {
    expect(formatTime(date, "nb-NO")).toBe("14:05");
  });

  it("uses a 12-hour clock for US English", () => {
    expect(formatTime(date, "en-US")).toMatch(/^02:05\s?PM$/);
  });

  it("includes weekday, day and month in the date", () => {
    const text = formatDate(date, "en-US");
    expect(text).toContain("Mon");
    expect(text).toContain("Nov");
    expect(text).toContain("16");
  });
});

import { formatDuration, formatPlayback, playbackPosition } from "./time";

describe("formatDuration", () => {
  it("uses hours and minutes as needed", () => {
    expect(formatDuration(20)).toBe("under a minute");
    expect(formatDuration(40 * 60)).toBe("40 min");
    expect(formatDuration(3 * 3600)).toBe("3 h");
    expect(formatDuration(3 * 3600 + 10 * 60 + 20)).toBe("3 h 10 min");
  });
});

describe("formatPlayback", () => {
  it("pads seconds, and minutes once there are hours", () => {
    expect(formatPlayback(0)).toBe("0:00");
    expect(formatPlayback(187_000)).toBe("3:07");
    expect(formatPlayback(3_765_000)).toBe("1:02:45");
    expect(formatPlayback(-5)).toBe("0:00");
  });
});

describe("playbackPosition", () => {
  const base = { positionMs: 60_000, durationMs: 200_000, updatedAtMs: 1_000_000 };

  it("advances with the clock while playing", () => {
    expect(playbackPosition({ ...base, playing: true }, 1_030_000)).toBe(90_000);
  });

  it("stays put while paused", () => {
    expect(playbackPosition({ ...base, playing: false }, 1_030_000)).toBe(60_000);
  });

  it("never passes the end, and tolerates a clock behind the report", () => {
    expect(playbackPosition({ ...base, playing: true }, 9_000_000)).toBe(200_000);
    expect(playbackPosition({ ...base, playing: true }, 900_000)).toBe(60_000);
  });

  it("does not extrapolate when the player never reported a time", () => {
    expect(playbackPosition({ ...base, updatedAtMs: 0, playing: true }, 5_000_000)).toBe(60_000);
  });
});
