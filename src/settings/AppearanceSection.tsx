import { useEffect, useRef, useState } from "react";
import type { Settings, SettingsPatch } from "../ipc/types";
import { GLASS_RANGES, THEMES, themeById, type Theme } from "../themes/themes";
import { useScheme } from "../themes/useAppearance";
import { Button, Row, Section, Segmented } from "./controls";

type Update = (patch: SettingsPatch) => Promise<void>;
type GlassKey = keyof typeof GLASS_RANGES;

const DEFAULT_ACCENT = "#2f6fed";

/** A small preview: the theme's tint over a stand-in for a wallpaper. */
function Swatch({ theme, scheme }: { theme: Theme; scheme: "light" | "dark" }) {
  const glass = theme.glass[scheme];
  const [r, g, b] = glass.tint.map((c) => Math.round(c * 255));
  const alpha = glass.opacity >= 1 ? 1 : Math.min(1, glass.tintStrength + glass.blur * 0.25);
  return (
    <span
      aria-hidden
      className="block h-9 rounded-lg"
      style={{
        backgroundImage: `linear-gradient(rgb(${r} ${g} ${b} / ${alpha}), rgb(${r} ${g} ${b} / ${alpha})), linear-gradient(115deg, #f3a35c, #d6508f 45%, #4d6fe0)`,
        boxShadow: "inset 0 1px 0 rgb(255 255 255 / 0.45), inset 0 0 0 1px rgb(255 255 255 / 0.12)",
      }}
    />
  );
}

/**
 * A slider that shows its value immediately but saves at most every 60 ms,
 * so dragging doesn't write the settings file on every pixel.
 */
function GlassSlider({
  name,
  value,
  disabled,
  onCommit,
}: {
  name: GlassKey;
  value: number;
  disabled: boolean;
  onCommit: (value: number) => void;
}) {
  const range = GLASS_RANGES[name];
  const [local, setLocal] = useState(value);
  const timer = useRef(0);
  const pending = useRef<number | null>(null);
  useEffect(() => {
    if (pending.current === null) setLocal(value);
  }, [value]);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const change = (next: number) => {
    setLocal(next);
    pending.current = next;
    if (timer.current) return;
    timer.current = window.setTimeout(() => {
      timer.current = 0;
      if (pending.current !== null) onCommit(pending.current);
      pending.current = null;
    }, 60);
  };

  return (
    <label className={`block px-4 py-3 ${disabled ? "opacity-45" : ""}`}>
      <span className="mb-2 flex justify-between">
        <span className="font-medium">{range.label}</span>
        <span className="tabular-nums text-[var(--text-muted)]">
          {range.step < 1 ? local.toFixed(2) : local}
        </span>
      </span>
      <input
        type="range"
        className="slider"
        min={range.min}
        max={range.max}
        step={range.step}
        value={local}
        disabled={disabled}
        onChange={(e) => change(Number(e.target.value))}
      />
    </label>
  );
}

export function Appearance({ settings, update }: { settings: Settings; update: Update }) {
  const { appearance } = settings;
  const scheme = useScheme(appearance.colorScheme);
  const theme = themeById(appearance.theme);
  const base = theme.glass[scheme];
  const opaque = base.opacity >= 1;
  const adjusted = Object.values(appearance.glass).some((v) => v !== null);

  return (
    <>
      <Section title="Theme">
        <div role="radiogroup" aria-label="Theme" className="grid grid-cols-3 gap-2 p-3">
          {THEMES.map((t) => {
            const selected = t.id === theme.id;
            return (
              <button
                key={t.id}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => update({ appearance: { theme: t.id } })}
                className={`rounded-xl border p-2 text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] ${
                  selected
                    ? "border-[var(--accent)] bg-[var(--surface-sunken)]"
                    : "border-transparent hover:bg-[var(--surface-sunken)]"
                }`}
              >
                <Swatch theme={t} scheme={scheme} />
                <span className="mt-2 block font-medium">{t.name}</span>
                <span className="block text-[12px] leading-snug text-[var(--text-muted)]">
                  {t.description}
                </span>
              </button>
            );
          })}
        </div>
      </Section>

      <Section title="Colour">
        <Row label="Light or dark" description="Follows Windows unless you choose one.">
          <Segmented
            label="Colour scheme"
            value={appearance.colorScheme}
            options={[
              { value: "system", label: "Windows" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
            onChange={(colorScheme) => update({ appearance: { colorScheme } })}
          />
        </Row>
        <Row label="Accent colour" description="Marks the active app, sliders and progress rings.">
          <div className="flex items-center gap-2">
            {appearance.accent && (
              <Button onClick={() => update({ appearance: { accent: null } })}>Use default</Button>
            )}
            <input
              type="color"
              aria-label="Accent colour"
              className="h-8 w-10 cursor-pointer rounded border border-[var(--border)] bg-transparent"
              value={appearance.accent ?? DEFAULT_ACCENT}
              onChange={(e) => update({ appearance: { accent: e.target.value } })}
            />
          </div>
        </Row>
      </Section>

      <Section title={opaque ? "Glass (this theme is opaque)" : "Glass"}>
        {(Object.keys(GLASS_RANGES) as GlassKey[]).map((name) => (
          <GlassSlider
            key={`${theme.id}-${scheme}-${name}`}
            name={name}
            disabled={opaque}
            value={appearance.glass[name] ?? base[name]}
            onCommit={(value) => update({ appearance: { glass: { [name]: value } } })}
          />
        ))}
        <Row label="Theme defaults" description="Clears the adjustments above.">
          <Button
            disabled={!adjusted}
            onClick={() =>
              update({
                appearance: {
                  glass: { refraction: null, edgeWidth: null, blur: null, chroma: null, highlight: null },
                },
              })
            }
          >
            Reset
          </Button>
        </Row>
      </Section>

      <Section title="Dock">
        <Row label="Size">
          <Segmented
            label="Dock size"
            value={appearance.dockSize}
            options={[
              { value: "small", label: "Small" },
              { value: "medium", label: "Medium" },
              { value: "large", label: "Large" },
            ]}
            onChange={(dockSize) => update({ appearance: { dockSize } })}
          />
        </Row>
      </Section>
    </>
  );
}
