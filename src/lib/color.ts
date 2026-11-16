/** Parse `#rrggbb` into 0..1 channels, or null if it isn't one. */
export function parseHex(hex: string): [number, number, number] | null {
  const match = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!match) return null;
  const value = parseInt(match[1], 16);
  return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255];
}

/** Relative luminance (WCAG) of an sRGB colour. */
export function luminance([r, g, b]: [number, number, number]): number {
  const linear = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

/** Black or white, whichever reads better on `hex`. */
export function inkOn(hex: string): "#000000" | "#ffffff" {
  const rgb = parseHex(hex);
  if (!rgb) return "#ffffff";
  // Equal contrast against black and white falls at a luminance of about 0.179.
  return luminance(rgb) > 0.179 ? "#000000" : "#ffffff";
}
