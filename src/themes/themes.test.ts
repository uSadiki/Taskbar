import { describe, expect, it } from "vitest";
import { THEMES, resolveGlass, themeById } from "./themes";

describe("themes", () => {
  it("has six themes with unique ids", () => {
    expect(THEMES).toHaveLength(6);
    expect(new Set(THEMES.map((t) => t.id)).size).toBe(6);
  });

  it("falls back to the default theme for an unknown id", () => {
    expect(themeById("nope").id).toBe("glass");
    expect(themeById("midnight").name).toBe("Midnight");
  });
});

describe("resolveGlass", () => {
  it("applies slider overrides to glass themes", () => {
    const params = resolveGlass(themeById("glass"), "dark", { refraction: 40, blur: 0.5 });
    expect(params.refraction).toBe(40);
    expect(params.blur).toBe(0.5);
    expect(params.edgeWidth).toBe(themeById("glass").glass.dark.edgeWidth);
  });

  it("ignores null and non-finite overrides", () => {
    const base = themeById("glass").glass.dark;
    const params = resolveGlass(themeById("glass"), "dark", { refraction: null, blur: Number.NaN });
    expect(params.refraction).toBe(base.refraction);
    expect(params.blur).toBe(base.blur);
  });

  it("leaves opaque themes alone", () => {
    const params = resolveGlass(themeById("white"), "light", { refraction: 40, blur: 1 });
    expect(params.refraction).toBe(0);
    expect(params.blur).toBe(0);
    expect(params.opacity).toBe(1);
  });

  it("uses the scheme's variant", () => {
    const glass = themeById("glass");
    expect(resolveGlass(glass, "light", {}).tint).toEqual([1, 1, 1]);
    expect(resolveGlass(glass, "dark", {}).tint).not.toEqual([1, 1, 1]);
  });
});
