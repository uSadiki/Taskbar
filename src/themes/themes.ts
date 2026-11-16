import type { GlassParams } from "../glass/renderer";
import type { Tone } from "../glass/luma";

export type Scheme = "light" | "dark";

export interface Theme {
  id: string;
  name: string;
  description: string;
  /** Glass look in each colour scheme. Opaque themes use the same in both. */
  glass: Record<Scheme, GlassParams>;
  /** "auto" picks light or dark content from the wallpaper behind the glass. */
  tone: Tone | "auto";
}

const CLEAR: GlassParams = {
  refraction: 24,
  edgeWidth: 15,
  blur: 0.1,
  chroma: 0.14,
  highlight: 0.5,
  rim: 0.55,
  fresnel: 0.2,
  saturation: 1.14,
  brightness: 1.02,
  tint: [0.05, 0.06, 0.09],
  tintStrength: 0.16,
  opacity: 0,
  shadow: 0.7,
};

const FROSTED: GlassParams = {
  ...CLEAR,
  refraction: 16,
  edgeWidth: 13,
  blur: 0.6,
  chroma: 0.08,
  highlight: 0.75,
  rim: 0.8,
  fresnel: 0.3,
  saturation: 1.3,
  brightness: 1.05,
  tintStrength: 0.3,
};

const solid = (tint: [number, number, number], shadow: number): GlassParams => ({
  ...CLEAR,
  refraction: 0,
  chroma: 0,
  blur: 0,
  highlight: 0.18,
  rim: 0.35,
  fresnel: 0.05,
  tint,
  opacity: 1,
  shadow,
});

const tinted = (tint: [number, number, number], overrides: Partial<GlassParams>): GlassParams => ({
  ...CLEAR,
  tint,
  ...overrides,
});

const both = (params: GlassParams): Record<Scheme, GlassParams> => ({ light: params, dark: params });

export const THEMES: Theme[] = [
  {
    id: "glass",
    name: "Glass",
    description: "Clear liquid glass that bends the wallpaper.",
    glass: {
      dark: CLEAR,
      light: { ...CLEAR, tint: [1, 1, 1], tintStrength: 0.14, brightness: 1.05 },
    },
    tone: "auto",
  },
  {
    id: "ios",
    name: "iOS Glass",
    description: "Lighter and more frosted, with brighter highlights.",
    glass: {
      dark: { ...FROSTED, tint: [0.1, 0.11, 0.14] },
      light: { ...FROSTED, tint: [1, 1, 1], tintStrength: 0.36 },
    },
    tone: "auto",
  },
  {
    id: "white",
    name: "White",
    description: "Opaque and light.",
    glass: both(solid([0.97, 0.975, 0.985], 0.55)),
    tone: "dark",
  },
  {
    id: "black",
    name: "Black",
    description: "Opaque and dark.",
    glass: both(solid([0.055, 0.058, 0.068], 0.8)),
    tone: "light",
  },
  {
    id: "midnight",
    name: "Midnight",
    description: "Deep blue-black tinted glass.",
    glass: both(
      tinted([0.02, 0.04, 0.13], {
        tintStrength: 0.84,
        blur: 0.4,
        refraction: 20,
        saturation: 1.3,
        brightness: 0.92,
        highlight: 0.42,
      }),
    ),
    tone: "light",
  },
  {
    id: "graphite",
    name: "Graphite",
    description: "Neutral smoked glass.",
    glass: both(
      tinted([0.13, 0.135, 0.145], {
        tintStrength: 0.78,
        blur: 0.45,
        refraction: 18,
        saturation: 0.55,
        highlight: 0.4,
        chroma: 0.05,
      }),
    ),
    tone: "light",
  },
];

export function themeById(id: string): Theme {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

/** The glass sliders in Settings. `null` or missing keeps the theme's value. */
export type GlassOverrides = Partial<
  Record<"refraction" | "edgeWidth" | "blur" | "chroma" | "highlight", number | null>
>;

/** A theme's glass with the user's slider overrides applied. */
export function resolveGlass(theme: Theme, scheme: Scheme, overrides: GlassOverrides): GlassParams {
  const params = { ...theme.glass[scheme] };
  // Opaque themes have nothing to refract or blur.
  if (params.opacity >= 1) return params;
  for (const key of ["refraction", "edgeWidth", "blur", "chroma", "highlight"] as const) {
    const value = overrides[key];
    if (typeof value === "number" && Number.isFinite(value)) params[key] = value;
  }
  return params;
}

/** Slider ranges, shared by Settings and the glass lab. */
export const GLASS_RANGES = {
  refraction: { min: 0, max: 60, step: 1, label: "Refraction" },
  edgeWidth: { min: 2, max: 40, step: 1, label: "Edge width" },
  blur: { min: 0, max: 1, step: 0.01, label: "Frost" },
  chroma: { min: 0, max: 0.6, step: 0.01, label: "Colour fringing" },
  highlight: { min: 0, max: 1.5, step: 0.01, label: "Highlight" },
} as const;
