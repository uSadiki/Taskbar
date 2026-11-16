import { useEffect, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { useApp } from "../stores/app";
import { resolveGlass, themeById, type GlassOverrides } from "../themes/themes";
import { useScheme } from "../themes/useAppearance";
import type { GlassParams } from "./renderer";

type NumericKey = Exclude<keyof GlassParams, "tint">;

const SLIDERS: { key: NumericKey; label: string; min: number; max: number; step: number }[] = [
  { key: "refraction", label: "Refraction strength", min: 0, max: 80, step: 1 },
  { key: "edgeWidth", label: "Edge width", min: 1, max: 60, step: 1 },
  { key: "blur", label: "Frost", min: 0, max: 1, step: 0.01 },
  { key: "chroma", label: "Chromatic aberration", min: 0, max: 1, step: 0.01 },
  { key: "highlight", label: "Highlight intensity", min: 0, max: 2, step: 0.01 },
  { key: "rim", label: "Rim line", min: 0, max: 2, step: 0.01 },
  { key: "fresnel", label: "Edge glow", min: 0, max: 1.5, step: 0.01 },
  { key: "saturation", label: "Saturation", min: 0, max: 2, step: 0.01 },
  { key: "brightness", label: "Brightness", min: 0.5, max: 1.5, step: 0.01 },
  { key: "tintStrength", label: "Tint strength", min: 0, max: 1, step: 0.01 },
  { key: "opacity", label: "Opacity", min: 0, max: 1, step: 0.01 },
  { key: "shadow", label: "Shadow", min: 0, max: 1.5, step: 0.01 },
];

const toHex = (tint: [number, number, number]) =>
  `#${tint.map((c) => Math.round(c * 255).toString(16).padStart(2, "0")).join("")}`;

const fromHex = (hex: string): [number, number, number] => [
  parseInt(hex.slice(1, 3), 16) / 255,
  parseInt(hex.slice(3, 5), 16) / 255,
  parseInt(hex.slice(5, 7), 16) / 255,
];

/**
 * Dev-only panel for tuning the glass. Every change is sent to the dock,
 * which draws with these values instead of the theme's until the lab closes.
 */
export function GlassLab() {
  const settings = useApp((s) => s.settings);
  const scheme = useScheme(settings?.appearance.colorScheme);
  const [params, setParams] = useState<GlassParams | null>(null);
  const [copied, setCopied] = useState(false);

  const themeId = settings?.appearance.theme;
  useEffect(() => {
    if (!settings || !themeId) return;
    const overrides = settings.appearance.glass as GlassOverrides;
    setParams(resolveGlass(themeById(themeId), scheme, overrides));
    // Restart from the theme whenever the theme or scheme changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [themeId, scheme]);

  useEffect(() => {
    if (params) void emit("glasslab://params", params);
  }, [params]);

  // Hand control back to the theme when the lab closes.
  useEffect(() => {
    const release = () => void emit("glasslab://params", null);
    window.addEventListener("beforeunload", release);
    return () => window.removeEventListener("beforeunload", release);
  }, []);

  if (!params) return <div className="p-6 text-[var(--text-muted)]">Loading</div>;

  const copy = async () => {
    await navigator.clipboard.writeText(JSON.stringify(params, null, 2));
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="h-full overflow-y-auto p-5">
      <h1 className="mb-1 text-lg font-semibold">Glass lab</h1>
      <p className="mb-4 text-[12.5px] text-[var(--text-muted)]">
        Changes apply to the dock as you drag. Open a popover to see them on a larger surface.
      </p>
      {SLIDERS.map(({ key, label, min, max, step }) => (
        <label key={key} className="mb-3 block">
          <span className="mb-1 flex justify-between text-[12.5px]">
            <span>{label}</span>
            <span className="tabular-nums text-[var(--text-muted)]">{params[key].toFixed(2)}</span>
          </span>
          <input
            type="range"
            className="slider"
            min={min}
            max={max}
            step={step}
            value={params[key]}
            onChange={(e) => setParams({ ...params, [key]: Number(e.target.value) })}
          />
        </label>
      ))}
      <label className="mb-4 flex items-center justify-between text-[12.5px]">
        <span>Tint colour</span>
        <input
          type="color"
          value={toHex(params.tint)}
          onChange={(e) => setParams({ ...params, tint: fromHex(e.target.value) })}
        />
      </label>
      <button
        type="button"
        onClick={() => void copy()}
        className="rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-1.5 text-[13px] font-medium hover:bg-[var(--surface-sunken)]"
      >
        {copied ? "Copied" : "Copy as theme JSON"}
      </button>
    </div>
  );
}
