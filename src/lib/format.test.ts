import { describe, expect, it } from "vitest";
import { ago, compactNumber, countdown, dollars, usageColor } from "./format";

describe("compactNumber", () => {
  it("shortens large numbers", () => {
    expect(compactNumber(950)).toBe("950");
    expect(compactNumber(1234)).toBe("1.2k");
    expect(compactNumber(3_400_000)).toBe("3.4M");
    expect(compactNumber(2_000_000)).toBe("2M");
    expect(compactNumber(250_000_000)).toBe("250M");
  });
});

describe("dollars", () => {
  it("keeps cents below a hundred", () => {
    expect(dollars(0.4234)).toBe("$0.42");
    expect(dollars(12.3)).toBe("$12.30");
    expect(dollars(148.7)).toBe("$149");
  });
});

describe("countdown", () => {
  const now = 1_000_000_000;
  it("uses the two largest units", () => {
    expect(countdown(now + 134 * 60_000, now)).toBe("2h 14m");
    expect(countdown(now + 38 * 60_000, now)).toBe("38m");
    expect(countdown(now + 120 * 60_000, now)).toBe("2h");
    expect(countdown(now + (4 * 1440 + 180) * 60_000, now)).toBe("4d 3h");
  });

  it("says now when the moment has passed", () => {
    expect(countdown(now - 5_000, now)).toBe("now");
  });
});

describe("ago", () => {
  const now = 1_000_000_000;
  it("describes elapsed time", () => {
    expect(ago(now - 10_000, now)).toBe("just now");
    expect(ago(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(ago(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(ago(now - 30 * 3_600_000, now)).toBe("yesterday");
    expect(ago(now - 72 * 3_600_000, now)).toBe("3 days ago");
  });
});

describe("usageColor", () => {
  it("changes at 80% and 95%", () => {
    expect(usageColor(50)).toBe("var(--accent)");
    expect(usageColor(80)).toBe("var(--warn)");
    expect(usageColor(97)).toBe("var(--bad)");
  });
});
