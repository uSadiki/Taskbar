import type { CssRect } from "../ipc/types";

/** Round to a tenth of a pixel so sub-pixel layout jitter doesn't cause updates. */
function round(value: number): number {
  return Math.round(value * 10) / 10;
}

export function toCssRect(rect: { left: number; top: number; width: number; height: number }): CssRect {
  return { x: round(rect.left), y: round(rect.top), width: round(rect.width), height: round(rect.height) };
}

/** Drop rectangles with no area; they would only add empty regions. */
export function visibleRects(rects: CssRect[]): CssRect[] {
  return rects.filter((r) => r.width > 0 && r.height > 0);
}

export function sameRects(a: CssRect[], b: CssRect[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (r, i) => r.x === b[i].x && r.y === b[i].y && r.width === b[i].width && r.height === b[i].height,
    )
  );
}
