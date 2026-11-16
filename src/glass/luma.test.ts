import { describe, expect, it } from "vitest";
import { buildLumaGrid, glassLuma, meanLuma, pickTone } from "./luma";

/** A `width` x `height` image: left half black, right half white. */
function halves(width: number, height: number): Uint8Array {
  const pixels = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const v = x < width / 2 ? 0 : 255;
      pixels.set([v, v, v, 255], (y * width + x) * 4);
    }
  }
  return pixels;
}

describe("buildLumaGrid", () => {
  it("averages each cell", () => {
    const grid = buildLumaGrid(halves(128, 64), 128, 64, 32);
    expect([grid.cols, grid.rows]).toEqual([4, 2]);
    expect(Array.from(grid.data)).toEqual([0, 0, 1, 1, 0, 0, 1, 1]);
  });

  it("handles sizes that are not a multiple of the cell", () => {
    const grid = buildLumaGrid(halves(70, 40), 70, 40, 32);
    expect([grid.cols, grid.rows]).toEqual([3, 2]);
    expect(grid.data[2]).toBe(1); // the 6px-wide last column is all white
  });
});

describe("meanLuma", () => {
  const grid = buildLumaGrid(halves(128, 64), 128, 64, 32);

  it("reads the cells under a rectangle", () => {
    expect(meanLuma(grid, 0, 0, 64, 64)).toBe(0);
    expect(meanLuma(grid, 64, 0, 128, 64)).toBe(1);
    expect(meanLuma(grid, 32, 0, 96, 32)).toBe(0.5);
  });

  it("clamps rectangles that extend past the image", () => {
    expect(meanLuma(grid, -500, -500, 40, 40)).toBe(0);
    expect(meanLuma(grid, 100, 10, 5000, 5000)).toBe(1);
  });
});

describe("glassLuma", () => {
  const clear = { brightness: 1, tint: [0, 0, 0] as [number, number, number], tintStrength: 0, opacity: 0 };

  it("is the wallpaper's luminance for clear glass", () => {
    expect(glassLuma(0.3, clear)).toBeCloseTo(0.3);
  });

  it("moves toward the tint", () => {
    expect(glassLuma(0.8, { ...clear, tintStrength: 0.5 })).toBeCloseTo(0.4);
    expect(glassLuma(0.2, { ...clear, tint: [1, 1, 1], tintStrength: 0.5 })).toBeCloseTo(0.6);
  });

  it("is the tint's luminance for opaque themes, whatever the wallpaper", () => {
    expect(glassLuma(0.1, { ...clear, tint: [1, 1, 1], opacity: 1 })).toBeCloseTo(1);
    expect(glassLuma(0.9, { ...clear, opacity: 1 })).toBeCloseTo(0);
  });
});

describe("pickTone", () => {
  it("puts dark content on light glass and light content on dark glass", () => {
    expect(pickTone(0.9, null)).toBe("dark");
    expect(pickTone(0.2, null)).toBe("light");
  });

  it("keeps the previous tone in the band between the thresholds", () => {
    expect(pickTone(0.56, "dark")).toBe("dark");
    expect(pickTone(0.56, "light")).toBe("light");
    expect(pickTone(0.56, null)).toBe("light");
  });
});
