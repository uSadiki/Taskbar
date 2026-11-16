import { describe, expect, it } from "vitest";
import { inkOn, luminance, parseHex } from "./color";

describe("parseHex", () => {
  it("parses six-digit colours", () => {
    expect(parseHex("#ff8000")).toEqual([1, 128 / 255, 0]);
    expect(parseHex(" #FFFFFF ")).toEqual([1, 1, 1]);
  });

  it("rejects anything else", () => {
    expect(parseHex("#fff")).toBeNull();
    expect(parseHex("red")).toBeNull();
    expect(parseHex("#12345g")).toBeNull();
  });
});

describe("luminance", () => {
  it("is 0 for black and 1 for white", () => {
    expect(luminance([0, 0, 0])).toBe(0);
    expect(luminance([1, 1, 1])).toBeCloseTo(1);
  });
});

describe("inkOn", () => {
  it("picks black on light colours and white on dark ones", () => {
    expect(inkOn("#ffffff")).toBe("#000000");
    expect(inkOn("#ffd54a")).toBe("#000000");
    expect(inkOn("#000000")).toBe("#ffffff");
    expect(inkOn("#1f3fa8")).toBe("#ffffff");
  });

  it("falls back to white for an invalid colour", () => {
    expect(inkOn("nope")).toBe("#ffffff");
  });
});
