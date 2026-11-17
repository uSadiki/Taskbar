import { describe, expect, it } from "vitest";
import { clockTime, compass, describeCode } from "./weatherCodes";

describe("describeCode", () => {
  it("maps WMO codes to an icon and a label", () => {
    expect(describeCode(0)).toEqual({ sky: "clear", label: "Clear" });
    expect(describeCode(2).sky).toBe("partly");
    expect(describeCode(45).sky).toBe("fog");
    expect(describeCode(63)).toEqual({ sky: "rain", label: "Rain" });
    expect(describeCode(67).sky).toBe("sleet");
    expect(describeCode(75).label).toBe("Heavy snow");
    expect(describeCode(81).sky).toBe("showers");
    expect(describeCode(86).sky).toBe("snow");
    expect(describeCode(99).sky).toBe("thunder");
  });

  it("falls back for unknown codes", () => {
    expect(describeCode(42).label).toBe("Unknown");
  });
});

describe("compass", () => {
  it("rounds to eight points", () => {
    expect(compass(0)).toBe("N");
    expect(compass(230)).toBe("SW");
    expect(compass(350)).toBe("N");
    expect(compass(-90)).toBe("W");
  });
});

describe("clockTime", () => {
  it("takes the time from a local ISO stamp", () => {
    expect(clockTime("2026-11-17T08:21")).toBe("08:21");
  });
});
