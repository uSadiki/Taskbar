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
