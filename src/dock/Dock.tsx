import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { useReducedMotion } from "motion/react";
import { GlassLayer } from "../glass/GlassLayer";
import type { GlassParams, GlassShape } from "../glass/renderer";
import { scene } from "../glass/scene";
import { inkOn } from "../lib/color";
import { useApp } from "../stores/app";
import { useAppearance, useTone } from "../themes/useAppearance";
import { Clock } from "./Clock";
import { usePopover } from "./popover";
import { PopoverHost } from "./PopoverHost";
import { useDockDrag } from "./useDockDrag";
import { useHitRegions } from "./useHitRegions";
import { WIDGETS } from "./widgets";

const PILL_RADIUS = 20;
/** How far the full-width bar's glass extends past the screen edges, so it has no rim there. */
const BLEED = 64;

export function Dock() {
  const layout = useApp((s) => s.layout);
  const settings = useApp((s) => s.settings);
  const barRef = useRef<HTMLDivElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const popoverId = usePopover((s) => s.id);
  const togglePopover = usePopover((s) => s.toggle);
  const closePopover = usePopover((s) => s.close);

  // Dev builds: the glass lab window streams parameters to try out.
  const [lab, setLab] = useState<GlassParams | null>(null);
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const unlisten = listen<GlassParams | null>("glasslab://params", (e) => setLab(e.payload));
    return () => void unlisten.then((fn) => fn());
  }, []);

  const appearance = useAppearance(settings, lab);
  scene.reducedMotion = useReducedMotion() ?? false;
  const onPointerDown = useDockDrag();

  useHitRegions([barRef, popoverRef], [layout, settings, popoverId]);

  // Keep the dock's glass on the bar. Size changes spring; a move to another
  // edge or layout jumps, because the whole window moved with it.
  const [barShape, setBarShape] = useState<GlassShape | null>(null);
  const placement = layout ? `${layout.edge}/${layout.layout}/${layout.windowHeight}` : "";
  const lastPlacement = useRef("");
  useLayoutEffect(() => {
    const bar = barRef.current;
    if (!bar || !layout) return;
    const measure = () => {
      const r = bar.getBoundingClientRect();
      if (r.width === 0) return;
      const shape: GlassShape =
        layout.layout === "full"
          ? {
              x: -BLEED,
              y: layout.edge === "bottom" ? r.top : r.top - BLEED,
              w: r.width + 2 * BLEED,
              h: r.height + BLEED,
              r: 0,
            }
          : { x: r.left, y: r.top, w: r.width, h: r.height, r: PILL_RADIUS };
      scene.set("dock", shape, { instant: lastPlacement.current !== placement });
      lastPlacement.current = placement;
      setBarShape(shape);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(bar);
    measure();
    return () => observer.disconnect();
    // `placement` stands in for the layout fields that matter here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [placement]);

  const tone = useTone(barShape, layout, appearance);

  if (!layout || !settings) return null;

  const openMenu = (event: MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    const bar = event.currentTarget.getBoundingClientRect();
    togglePopover("menu", { x: event.clientX - 22, y: bar.top + 6, w: 44, h: bar.height - 12 });
  };

  const tileSize = layout.dockHeight - 12;
  const enabled = settings.widgets.filter((w) => w.enabled);
  const fallback: [number, number, number] =
    appearance.scheme === "dark" ? [0.15, 0.16, 0.19] : [0.9, 0.91, 0.93];

  // A user-chosen accent overrides the per-tone default on every surface.
  const accent = settings.appearance.accent;
  const accentVars = accent
    ? ({ "--accent-user": accent, "--accent-user-ink": inkOn(accent) } as CSSProperties)
    : undefined;

  return (
    <div className="pointer-events-none fixed inset-0" style={accentVars}>
      <GlassLayer params={appearance.glass} layout={layout} fallback={fallback} />

      {/* The reserved strip, at the window edge that touches the screen edge. */}
      <div
        className="absolute inset-x-0 flex justify-center"
        style={{ [layout.edge]: 0, height: layout.bandHeight, padding: layout.gap }}
      >
        <div
          ref={barRef}
          className="dock-bar glass-content pointer-events-auto"
          data-layout={layout.layout}
          data-tone={tone}
          style={{ height: layout.dockHeight }}
          onPointerDown={onPointerDown}
          onContextMenu={openMenu}
          onClick={(e) => {
            if (e.target === e.currentTarget) closePopover();
          }}
        >
          {enabled.map(({ id }) => {
            const { label, Icon, phase } = WIDGETS[id];
            return (
              <button
                key={id}
                type="button"
                className="dock-tile"
                style={{ width: tileSize, height: tileSize }}
                title={`${label} (arrives in phase ${phase})`}
                aria-label={label}
              >
                <Icon size={Math.round(tileSize * 0.5)} strokeWidth={1.75} aria-hidden />
              </button>
            );
          })}
          {layout.layout === "full" && <div className="flex-1" />}
          {layout.safeMode && (
            <span className="dock-badge" title="Safe mode: no AppBar, taskbar untouched">
              Safe mode
            </span>
          )}
          <Clock />
        </div>
      </div>

      <PopoverHost layout={layout} appearance={appearance} hostRef={popoverRef} />
    </div>
  );
}
