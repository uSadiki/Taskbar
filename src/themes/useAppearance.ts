import { useEffect, useMemo, useRef, useState } from "react";
import { useGlass } from "../glass/GlassLayer";
import { glassLuma, meanLuma, pickTone, type Tone } from "../glass/luma";
import type { GlassParams, GlassShape } from "../glass/renderer";
import type { DockLayout, Settings } from "../ipc/types";
import { resolveGlass, themeById, type GlassOverrides, type Scheme, type Theme } from "./themes";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/** The colour scheme in effect: the setting, or Windows' when set to follow it. */
export function useScheme(setting: Settings["appearance"]["colorScheme"] | undefined): Scheme {
  const [systemDark, setSystemDark] = useState(() => window.matchMedia(DARK_QUERY).matches);
  useEffect(() => {
    const query = window.matchMedia(DARK_QUERY);
    const onChange = () => setSystemDark(query.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);
  const scheme = setting === "light" || setting === "dark" ? setting : systemDark ? "dark" : "light";
  useEffect(() => {
    document.documentElement.dataset.scheme = scheme;
  }, [scheme]);
  return scheme;
}

export interface Appearance {
  theme: Theme;
  scheme: Scheme;
  glass: GlassParams;
}

/**
 * The glass parameters in effect: the theme for the current scheme, the
 * user's slider overrides, and (in dev builds) live values from the glass lab.
 */
export function useAppearance(settings: Settings | null, lab: GlassParams | null): Appearance {
  const scheme = useScheme(settings?.appearance.colorScheme);
  const theme = themeById(settings?.appearance.theme ?? "glass");
  const overrides = settings?.appearance.glass as GlassOverrides | undefined;
  const glass = useMemo(
    () => lab ?? resolveGlass(theme, scheme, overrides ?? {}),
    [lab, theme, scheme, overrides],
  );
  return { theme, scheme, glass };
}

/**
 * Light or dark content for a glass surface, from the wallpaper behind it.
 * `shape` is in CSS pixels relative to the window.
 */
export function useTone(
  shape: GlassShape | null,
  layout: DockLayout | null,
  appearance: Appearance,
): Tone {
  const grid = useGlass((s) => s.grid);
  const previous = useRef<Tone | null>(null);
  const { theme, glass, scheme } = appearance;

  let tone: Tone;
  if (theme.tone !== "auto") {
    tone = theme.tone;
  } else if (!grid || !shape || !layout) {
    // No wallpaper to measure: the fallback fill follows the colour scheme.
    tone = previous.current ?? (scheme === "dark" ? "light" : "dark");
  } else {
    const s = layout.scale;
    const x0 = layout.originX + shape.x * s;
    const y0 = layout.originY + shape.y * s;
    const wallpaper = meanLuma(grid, x0, y0, x0 + shape.w * s, y0 + shape.h * s);
    tone = pickTone(glassLuma(wallpaper, glass), previous.current);
  }
  previous.current = tone;
  return tone;
}
