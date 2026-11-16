import { describe, expect, it } from "vitest";
import { isoWeek, monthGrid, sameDay, weekStartFor, weekdayNames } from "./calendar";

describe("weekStartFor", () => {
  it("is Monday in Norway and Sunday in the US", () => {
    expect(weekStartFor("nb-NO")).toBe(1);
    expect(weekStartFor("en-US")).toBe(0);
  });

  it("falls back to Monday for nonsense", () => {
    expect(weekStartFor("not a locale!")).toBe(1);
  });
});

describe("monthGrid", () => {
  it("starts November 2026 on Monday 26 October with a Monday week start", () => {
    // 1 November 2026 is a Sunday.
    const grid = monthGrid(2026, 10, 1);
    expect(grid).toHaveLength(6);
    expect(grid.every((week) => week.length === 7)).toBe(true);
    expect(grid[0][0]).toEqual(new Date(2026, 9, 26));
    expect(grid[0][6]).toEqual(new Date(2026, 10, 1));
    expect(grid[5][6]).toEqual(new Date(2026, 11, 6));
  });

  it("starts the same month on 1 November with a Sunday week start", () => {
    expect(monthGrid(2026, 10, 0)[0][0]).toEqual(new Date(2026, 10, 1));
  });

  it("contains every day of the month exactly once", () => {
    const days = monthGrid(2024, 1, 1).flat().filter((d) => d.getMonth() === 1);
    expect(days.map((d) => d.getDate())).toEqual(Array.from({ length: 29 }, (_, i) => i + 1));
  });
});

describe("isoWeek", () => {
  it("handles year boundaries", () => {
    expect(isoWeek(new Date(2026, 10, 16))).toBe(47);
    expect(isoWeek(new Date(2026, 0, 1))).toBe(1); // a Thursday
    expect(isoWeek(new Date(2027, 0, 1))).toBe(53); // a Friday: still week 53 of 2026
    expect(isoWeek(new Date(2024, 11, 30))).toBe(1); // Monday of week 1 of 2025
  });
});

describe("helpers", () => {
  it("compares calendar days, ignoring time", () => {
    expect(sameDay(new Date(2026, 10, 16, 1), new Date(2026, 10, 16, 23))).toBe(true);
    expect(sameDay(new Date(2026, 10, 16), new Date(2026, 10, 17))).toBe(false);
  });

  it("orders weekday names from the week start", () => {
    expect(weekdayNames(1, "en-US")).toEqual(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
    expect(weekdayNames(0, "en-US")[0]).toBe("Sun");
  });
});
