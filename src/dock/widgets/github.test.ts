import { describe, expect, it } from "vitest";
import type { ContributionDay } from "../../ipc/types";
import { lastWeeks } from "./Github";

const day = (date: string, count = 1): ContributionDay => ({ date, count, level: count > 0 ? 1 : 0 });

describe("lastWeeks", () => {
  it("splits days into Monday-first weeks and pads the first one", () => {
    // 2026-11-11 is a Wednesday.
    const days = ["11", "12", "13", "14", "15", "16", "17"].map((d) => day(`2026-11-${d}`));
    const weeks = lastWeeks(days, 10);
    expect(weeks).toHaveLength(2);
    expect(weeks[0].slice(0, 2)).toEqual([null, null]);
    expect(weeks[0][2]?.date).toBe("2026-11-11");
    expect(weeks[0][6]?.date).toBe("2026-11-15"); // Sunday
    expect(weeks[1].map((d) => d?.date)).toEqual(["2026-11-16", "2026-11-17"]);
  });

  it("keeps only the most recent weeks", () => {
    const days: ContributionDay[] = [];
    for (let i = 0; i < 70; i++) {
      days.push(day(new Date(Date.UTC(2026, 0, 5 + i)).toISOString().slice(0, 10)));
    }
    const weeks = lastWeeks(days, 4);
    expect(weeks).toHaveLength(4);
    expect(weeks[3].at(-1)?.date).toBe(days.at(-1)?.date);
  });

  it("returns nothing for no days", () => {
    expect(lastWeeks([], 18)).toEqual([]);
  });
});
