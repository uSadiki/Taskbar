// Brightness of the wallpaper behind a glass surface, used to choose light or
// dark content so text stays readable on any wallpaper.

/** Mean luminance (0..1) of the wallpaper per square cell of device pixels. */
export interface LumaGrid {
  cols: number;
  rows: number;
  cell: number;
  data: Float32Array;
}

/** Content colour: "light" is light text for dark glass, "dark" the reverse. */
export type Tone = "light" | "dark";

/** Build the grid from RGBA pixels. Samples every fourth pixel of each cell. */
export function buildLumaGrid(pixels: Uint8Array, width: number, height: number, cell = 32): LumaGrid {
  const cols = Math.ceil(width / cell);
  const rows = Math.ceil(height / cell);
  const data = new Float32Array(cols * rows);
  const step = 4;
  for (let row = 0; row < rows; row++) {
    for (let col = 0; col < cols; col++) {
      let sum = 0;
      let count = 0;
      const yEnd = Math.min((row + 1) * cell, height);
      const xEnd = Math.min((col + 1) * cell, width);
      for (let y = row * cell; y < yEnd; y += step) {
        for (let x = col * cell; x < xEnd; x += step) {
          const i = (y * width + x) * 4;
          sum += 0.2126 * pixels[i] + 0.7152 * pixels[i + 1] + 0.0722 * pixels[i + 2];
          count++;
        }
      }
      data[row * cols + col] = count ? sum / count / 255 : 0;
    }
  }
  return { cols, rows, cell, data };
}

/** Mean luminance of the cells a device-pixel rectangle touches. */
export function meanLuma(grid: LumaGrid, x0: number, y0: number, x1: number, y1: number): number {
  const c0 = Math.max(0, Math.floor(x0 / grid.cell));
  const r0 = Math.max(0, Math.floor(y0 / grid.cell));
  const c1 = Math.min(grid.cols - 1, Math.floor((x1 - 1) / grid.cell));
  const r1 = Math.min(grid.rows - 1, Math.floor((y1 - 1) / grid.cell));
  let sum = 0;
  let count = 0;
  for (let r = r0; r <= r1; r++) {
    for (let c = c0; c <= c1; c++) {
      sum += grid.data[r * grid.cols + c];
      count++;
    }
  }
  return count ? sum / count : 0.5;
}

interface ToneInputs {
  brightness: number;
  tint: [number, number, number];
  tintStrength: number;
  opacity: number;
}

/** Luminance of the glass after the theme's brightness and tint are applied. */
export function glassLuma(wallpaperLuma: number, params: ToneInputs): number {
  const tint = 0.2126 * params.tint[0] + 0.7152 * params.tint[1] + 0.0722 * params.tint[2];
  const lit = Math.min(1, wallpaperLuma * params.brightness);
  const tinted = lit + (tint - lit) * params.tintStrength;
  return tinted + (tint - tinted) * params.opacity;
}

/**
 * Light glass gets dark content and the reverse. Between the two thresholds
 * the previous choice is kept, so content doesn't flicker on a wallpaper that
 * sits near the boundary.
 */
export function pickTone(luma: number, previous: Tone | null): Tone {
  if (luma >= 0.62) return "dark";
  if (luma <= 0.5) return "light";
  return previous ?? "light";
}
