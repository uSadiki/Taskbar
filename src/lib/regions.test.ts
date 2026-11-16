import { describe, expect, it } from "vitest";
import { sameRects, toCssRect, visibleRects } from "./regions";

describe("toCssRect", () => {
  it("converts a DOMRect-like object and rounds to tenths", () => {
    expect(toCssRect({ left: 10.04, top: 20.26, width: 100.049, height: 52 })).toEqual({
      x: 10,
      y: 20.3,
      width: 100,
      height: 52,
    });
  });
});

describe("visibleRects", () => {
  it("drops zero-area rectangles", () => {
    const rects = [
      { x: 0, y: 0, width: 10, height: 10 },
      { x: 0, y: 0, width: 0, height: 10 },
      { x: 0, y: 0, width: 10, height: 0 },
    ];
    expect(visibleRects(rects)).toEqual([rects[0]]);
  });
});

describe("sameRects", () => {
  const a = { x: 1, y: 2, width: 3, height: 4 };

  it("is true for equal lists", () => {
    expect(sameRects([a], [{ ...a }])).toBe(true);
    expect(sameRects([], [])).toBe(true);
  });

  it("is false when a value or the length differs", () => {
    expect(sameRects([a], [{ ...a, width: 5 }])).toBe(false);
    expect(sameRects([a], [a, a])).toBe(false);
  });
});
