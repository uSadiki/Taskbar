import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { AnimatePresence, motion } from "motion/react";
import type { GlassShape } from "../glass/renderer";
import { scene } from "../glass/scene";
import { api, events, report } from "../ipc/api";
import type { DockLayout } from "../ipc/types";
import { useTone, type Appearance } from "../themes/useAppearance";
import { usePopover, type Anchor } from "./popover";
import { AppMenuPopover } from "./popovers/AppMenuPopover";
import { CalendarPopover } from "./popovers/CalendarPopover";
import { MenuPopover } from "./popovers/MenuPopover";
import { BatteryPopover } from "./widgets/Battery";
import { NowPlayingPopover } from "./widgets/NowPlaying";
import { QuickTogglesPopover } from "./widgets/QuickToggles";

interface PopoverEntry {
  /** Width in CSS pixels; the height follows the content. */
  width: number;
  render: () => ReactNode;
}

/** The popovers with a fixed id. */
const POPOVERS: Record<string, PopoverEntry> = {
  menu: { width: 208, render: () => <MenuPopover /> },
  clock: { width: 304, render: () => <CalendarPopover /> },
  nowPlaying: { width: 300, render: () => <NowPlayingPopover /> },
  battery: { width: 280, render: () => <BatteryPopover /> },
  toggles: { width: 324, render: () => <QuickTogglesPopover /> },
};

/** Find the popover for an id. Ids of the form `app:<id>` are per-app menus. */
function resolve(id: string | null): PopoverEntry | undefined {
  if (!id) return undefined;
  if (id.startsWith("app:")) {
    const appId = id.slice(4);
    return { width: 264, render: () => <AppMenuPopover appId={appId} /> };
  }
  return POPOVERS[id];
}

const GAP = 10; // between the dock and the popover
const RADIUS = 22;
const TILE_RADIUS = 12;
const MARGIN = 12; // minimum distance from the window's side edges
/** Smooth-union distance while the popover is still attached to the dock. */
const BLEND = 30;
/** Popovers carry text, so their glass is frosted more than the dock's. */
const FROST = 0.5;

const sameShape = (a: GlassShape | null, b: GlassShape) =>
  !!a && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;

const tileShape = (anchor: Anchor): GlassShape => ({ ...anchor, r: TILE_RADIUS });

interface PopoverHostProps {
  layout: DockLayout;
  appearance: Appearance;
  /** The element whose rectangle is reported as clickable while a popover is open. */
  hostRef: RefObject<HTMLDivElement | null>;
}

/**
 * Renders the open popover and its glass. The glass starts as the tile the
 * popover was opened from and springs out to the popover's size, merged with
 * the dock on the way, so it reads as a droplet leaving the dock.
 */
export function PopoverHost({ layout, appearance, hostRef }: PopoverHostProps) {
  const id = usePopover((s) => s.id);
  const anchor = usePopover((s) => s.anchor);
  const close = usePopover((s) => s.close);
  const entry = useMemo(() => resolve(id), [id]);
  const open = !!entry && !!anchor;

  const [shape, setShape] = useState<GlassShape | null>(null);
  const target = useRef<GlassShape | null>(null);
  // Geometry is kept while the content fades out after closing.
  const geometry = useRef({ left: 0, width: 0 });
  if (entry && anchor) {
    const max = Math.max(MARGIN, layout.windowWidth - entry.width - MARGIN);
    geometry.current = {
      width: entry.width,
      left: Math.min(Math.max(anchor.x + anchor.w / 2 - entry.width / 2, MARGIN), max),
    };
  }
  const { left, width } = geometry.current;
  const offset = layout.bandHeight - layout.gap + GAP;

  useLayoutEffect(() => {
    const element = hostRef.current;
    if (!open || !element || !anchor) {
      if (target.current && anchor) {
        scene.remove("popover", { to: tileShape(anchor), blend: [0, BLEND] });
      } else {
        scene.remove("popover");
      }
      target.current = null;
      setShape(null);
      return;
    }
    const measure = () => {
      const next: GlassShape = {
        x: element.offsetLeft,
        y: element.offsetTop,
        w: element.offsetWidth,
        h: element.offsetHeight,
        r: RADIUS,
        frost: FROST,
      };
      if (next.h === 0 || sameShape(target.current, next)) return;
      scene.set(
        "popover",
        next,
        target.current ? { blend: [0, 0] } : { from: tileShape(anchor), blend: [BLEND, 0] },
      );
      target.current = next;
      setShape(next);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, [open, anchor, hostRef, left, width, offset, layout.edge]);

  // The dock never takes focus on its own. Take it while a popover is open,
  // so Esc works and clicking another window closes the popover.
  useEffect(() => {
    if (!open) return;
    api.dockFocus().catch(report("dock_focus"));
    let unlisten: (() => void) | undefined;
    let ended = false;
    void events.dockBlur(close).then((fn) => (ended ? fn() : (unlisten = fn)));
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      ended = true;
      unlisten?.();
      window.removeEventListener("keydown", onKey);
      api.dockReleaseFocus().catch(report("dock_release_focus"));
    };
  }, [open, close]);

  const tone = useTone(shape, layout, appearance);

  return (
    <div
      ref={hostRef}
      data-tone={tone}
      className="glass-content pointer-events-auto absolute"
      style={{ left, width, [layout.edge]: offset }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <AnimatePresence mode="wait">
        {open && entry && (
          <motion.div
            key={id}
            style={{ transformOrigin: layout.edge === "bottom" ? "50% 100%" : "50% 0%" }}
            initial={{ opacity: 0, scale: 0.9 }}
            animate={{
              opacity: 1,
              scale: 1,
              transition: { type: "spring", stiffness: 420, damping: 34, delay: 0.03 },
            }}
            exit={{ opacity: 0, scale: 0.96, transition: { duration: 0.09 } }}
          >
            {entry.render()}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
